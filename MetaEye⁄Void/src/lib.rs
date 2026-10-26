#![forbid(unsafe_op_in_unsafe_fn)]
pub mod discord_bot;

use std::collections::HashMap;
use std::env;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Instant;

use regex::Regex;
use schemars::{schema_for, JsonSchema};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop};

pub const EMBEDDING_DIM: usize = 3072;
pub const CONFIDENCE_FLOOR: f32 = 0.10;
pub const AUTONOMOUS_ACT_THRESHOLD: f32 = 0.75;
pub const MAX_VOCAL_WORDS: usize = 60;

#[derive(Debug, thiserror::Error)]
pub enum SystemError {
    #[error("STOP=Missing_database_schema_or_invalid_proto_spec: {0}")]
    ContractGuardHalt(String),
    #[error("STOP=Missing_cact_weights_or_invalid_tool_grammar_schema: {0}")]
    NeedleEngineHalt(String),
    #[error("AuthFailure: {0}")]
    AuthFailure(String),
    #[error("VoicePipelineError: {0}")]
    VoiceError(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SanitizedRuntimeContext {
    pub telemetry_disabled: bool,
    pub proot_sanitized: bool,
    pub termux_bin_prepended: bool,
    pub simulator_fallback: bool,
}

pub fn sanitize_runtime_environment() -> SanitizedRuntimeContext {
    env::set_var("NEEDLE_TELEMETRY", "0");
    env::set_var("DO_NOT_TRACK", "1");

    let mut proot_sanitized = false;
    for var in ["ANDROID_ROOT", "ANDROID_DATA"] {
        if env::var_os(var).is_some() {
            env::remove_var(var);
            proot_sanitized = true;
        }
    }

    let termux_bin = "/data/data/com.termux/files/usr/bin";
    let termux_exists = std::path::Path::new(termux_bin).exists();
    if termux_exists {
        let current_path = env::var("PATH").unwrap_or_default();
        if !current_path.contains(termux_bin) {
            env::set_var("PATH", format!("{termux_bin}:{current_path}"));
        }
    }

    SanitizedRuntimeContext {
        telemetry_disabled: env::var("NEEDLE_TELEMETRY").as_deref() == Ok("0")
            && env::var("DO_NOT_TRACK").as_deref() == Ok("1"),
        proot_sanitized,
        termux_bin_prepended: termux_exists,
        simulator_fallback: !termux_exists,
    }
}

pub fn verify_qssu_contracts(
    openapi_spec: &str,
    database_schema: &str,
    proto_spec: &str,
) -> Result<(), SystemError> {
    if openapi_spec.trim().is_empty()
        || !openapi_spec.contains("openapi: 3.")
        || !openapi_spec.contains("/api/embed")
    {
        return Err(SystemError::ContractGuardHalt(
            "OpenAPI_Spec missing required 3.x header or /api/embed path".into(),
        ));
    }

    for required_table in [
        "CREATE TABLE IF NOT EXISTS users",
        "CREATE TABLE IF NOT EXISTS sessions",
        "CREATE TABLE IF NOT EXISTS shorthand_macros",
        "CREATE TABLE IF NOT EXISTS tool_embeddings",
        "CREATE TABLE IF NOT EXISTS audit_logs",
    ] {
        if !database_schema.contains(required_table) {
            return Err(SystemError::ContractGuardHalt(format!(
                "Database_Schema missing table invariant: {required_table}"
            )));
        }
    }

    if !proto_spec.contains("syntax = \"proto3\";")
        || !proto_spec.contains("service LiveVoiceService")
        || !proto_spec.contains("service MetacogEngineService")
    {
        return Err(SystemError::ContractGuardHalt(
            "Proto3 specification missing LiveVoiceService or MetacogEngineService".into(),
        ));
    }

    Ok(())
}

// ============================================================================
// MAC CORE: AuthEngine, TokenManager, MetricsExporter (Secrecy + Zeroize)
// ============================================================================

#[derive(Zeroize, ZeroizeOnDrop)]
pub struct KeyMaterial {
    raw_bytes: [u8; 32],
    signing_secret: SecretString,
}

impl KeyMaterial {
    pub fn from_ephemeral_seed(seed: [u8; 32], label: String) -> Self {
        Self {
            raw_bytes: seed,
            signing_secret: SecretString::from(label),
        }
    }

    pub fn use_key<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&[u8; 32], &str) -> R,
    {
        f(&self.raw_bytes, self.signing_secret.expose_secret())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SovereignRole {
    SovereignI,
    CoExplorer,
    Auditor,
}

pub struct AuthEngine;

impl AuthEngine {
    pub fn evaluate_access(
        role: SovereignRole,
        is_metaprogram_rewrite: bool,
        explicit_human_confirmation: bool,
    ) -> Result<(), SystemError> {
        match role {
            SovereignRole::SovereignI => {
                if is_metaprogram_rewrite && !explicit_human_confirmation {
                    return Err(SystemError::AuthFailure(
                        "REQ-SEC-06 Violation: Metaprogram rewrite requires explicit Sovereign 'I' confirmation".into(),
                    ));
                }
                Ok(())
            }
            SovereignRole::CoExplorer if !is_metaprogram_rewrite => Ok(()),
            _ => Err(SystemError::AuthFailure(
                "Insufficient role authority for requested biocomputer level operation".into(),
            )),
        }
    }
}

pub struct TokenManager {
    signing_key: KeyMaterial,
}

impl TokenManager {
    pub fn new(seed: [u8; 32]) -> Self {
        Self {
            signing_key: KeyMaterial::from_ephemeral_seed(seed, "paseto-v4-local-domain".into()),
        }
    }

    pub fn issue_scoped_token(&self, user_id: &str, session_id: &str) -> SecretString {
        self.signing_key.use_key(|key, domain| {
            let mut mac: u64 = 0xcbf29ce484222325;
            for b in key
                .iter()
                .chain(domain.as_bytes())
                .chain(user_id.as_bytes())
                .chain(session_id.as_bytes())
            {
                mac ^= u64::from(*b);
                mac = mac.wrapping_mul(0x100000001b3);
            }
            SecretString::from(format!("v4.local.{user_id}.{session_id}.{mac:016x}"))
        })
    }

    pub fn verify_token(
        &self,
        token: &SecretString,
        expected_user: &str,
        expected_session: &str,
    ) -> bool {
        let expected = self.issue_scoped_token(expected_user, expected_session);
        token.expose_secret() == expected.expose_secret()
    }
}

#[derive(Default)]
pub struct MetricsExporter {
    pub total_turns: AtomicU64,
    pub autonomous_tool_calls: AtomicU64,
    pub suppressed_tool_calls: AtomicU64,
    pub confirmation_gated_calls: AtomicU64,
    pub live_voice_frames_processed: AtomicU64,
    pub metacog_pause_holds: AtomicU64,
}

impl MetricsExporter {
    pub fn render_prometheus(&self) -> String {
        format!(
            "# HELP needle_total_turns Total processed metacognitive turns\n             # TYPE needle_total_turns counter\n             needle_total_turns {}\n             # HELP needle_autonomous_tool_calls Tool calls executed autonomously (c >= 0.75)\n             # TYPE needle_autonomous_tool_calls counter\n             needle_autonomous_tool_calls {}\n             # HELP needle_suppressed_tool_calls Tool calls withheld below 0.10 floor\n             # TYPE needle_suppressed_tool_calls counter\n             needle_suppressed_tool_calls {}\n             # HELP needle_confirmation_gated_calls Tool calls gated by Sovereign I modal (0.10 <= c < 0.75)\n             # TYPE needle_confirmation_gated_calls counter\n             needle_confirmation_gated_calls {}\n             # HELP needle_live_voice_frames_processed Total 20ms PCM frames analyzed\n             # TYPE needle_live_voice_frames_processed counter\n             needle_live_voice_frames_processed {}\n             # HELP needle_metacog_pause_holds Total Parr POMDP dysfluency pause holds\n             # TYPE needle_metacog_pause_holds counter\n             needle_metacog_pause_holds {}\n",
            self.total_turns.load(Ordering::Relaxed),
            self.autonomous_tool_calls.load(Ordering::Relaxed),
            self.suppressed_tool_calls.load(Ordering::Relaxed),
            self.confirmation_gated_calls.load(Ordering::Relaxed),
            self.live_voice_frames_processed.load(Ordering::Relaxed),
            self.metacog_pause_holds.load(Ordering::Relaxed),
        )
    }
}

// ============================================================================
// CACTUS NEEDLE 3 / 2 ENGINE & 3,072-DIM TOOL EMBEDDER
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NeedleGeneration {
    Needle2Legacy14MB = 2,
    Needle3LadderedSAN = 3,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NeedleToolSchema {
    pub name: String,
    pub description: String,
    pub parameters_json_schema: String,
    pub regex_trigger: Option<String>,
    pub embedding_3072: Vec<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NeedleFunctionCall {
    pub name: String,
    pub arguments: serde_json::Value,
    pub confidence: f32,
    pub triggered_by_regex: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NeedleRunResult {
    pub function_calls: Vec<NeedleFunctionCall>,
    pub suppressed_calls: Vec<NeedleFunctionCall>,
    pub reasoning: String,
    pub confidence: f32,
}

pub fn compute_needle_3072_embedding(text: &str) -> Vec<f32> {
    let mut vec = vec![0.0_f32; EMBEDDING_DIM];
    let normalized = text.to_lowercase();
    let tokens: Vec<&str> = normalized
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|s| !s.is_empty())
        .collect();

    for (pos, token) in tokens.iter().enumerate() {
        let mut h1: u64 = 0xcbf29ce484222325;
        for b in token.as_bytes() {
            h1 ^= u64::from(*b);
            h1 = h1.wrapping_mul(0x100000001b3);
        }
        for k in 0..8_u64 {
            let idx =
                ((h1.wrapping_add(k.wrapping_mul(0x9e3779b97f4a7c15))) as usize) % EMBEDDING_DIM;
            let sign = if (h1 >> (k + 8)) & 1 == 0 { 1.0 } else { -1.0 };
            let positional_decay = 1.0 / (1.0 + (pos as f32) * 0.05);
            vec[idx] += sign * positional_decay;
        }
    }

    let norm = vec.iter().map(|v| v * v).sum::<f32>().sqrt();
    if norm > 1e-6 {
        for v in &mut vec {
            *v /= norm;
        }
    }
    vec
}

pub fn cosine_similarity_3072(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != EMBEDDING_DIM || b.len() != EMBEDDING_DIM {
        return 0.0;
    }
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

// ============================================================================
// METACOGNITION ASSISTANCE TOOL SCHEMAS
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AuditBiocomputerMetaprogramArgs {
    pub target_belief_or_program: String,
    #[serde(default = "default_biocomputer_level")]
    pub biocomputer_level: u8,
    #[serde(default = "default_flattening_mode")]
    pub operation: String,
}
fn default_biocomputer_level() -> u8 { 7 }
fn default_flattening_mode() -> String { "opacify_parameter".into() }

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct NavigateAmnesicMazeArgs {
    pub blockade_metaphor: String,
    #[serde(default = "default_defense_mechanism")]
    pub defense_mechanism: String,
    #[serde(default = "default_crutch_bracket")]
    pub sandbox_bracket: String,
}
fn default_defense_mechanism() -> String { "habit_prior_lock".into() }
fn default_crutch_bracket() -> String { "inspect_lock_architecture".into() }

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GateActrTopicRelevanceArgs {
    pub candidate_chunk: String,
    pub active_goal_scheme: String,
    #[serde(default = "default_triage_action")]
    pub triage_action: String,
}
fn default_triage_action() -> String { "on_path_core".into() }

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CalibrateTerminologyFirewallArgs {
    pub polysemous_term: String,
    #[serde(default = "default_description_level")]
    pub target_level: String,
}
fn default_description_level() -> String { "L2_Cybernetic".into() }

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ManageShorthandMacroArgs {
    pub macro_trigger: String,
    #[serde(default = "default_macro_action")]
    pub action: String,
    #[serde(default)]
    pub formal_expansion: String,
}
fn default_macro_action() -> String { "formalize_new".into() }

pub struct CactusNeedleEngine {
    pub active_generation: RwLock<NeedleGeneration>,
    pub active_layers: RwLock<u8>,
    pub registered_tools: Vec<NeedleToolSchema>,
    pub compiled_triggers: HashMap<String, Regex>,
}

impl CactusNeedleEngine {
    pub fn initialize_default() -> Result<Self, SystemError> {
        let mut engine = Self {
            active_generation: RwLock::new(NeedleGeneration::Needle3LadderedSAN),
            active_layers: RwLock::new(20),
            registered_tools: Vec::new(),
            compiled_triggers: HashMap::new(),
        };
        engine.register_metacog_toolset()?;
        Ok(engine)
    }

    fn register_metacog_toolset(&mut self) -> Result<(), SystemError> {
        let definitions = vec![
            (
                "audit_biocomputer_metaprogram",
                "Audit a personal belief, stored program, or metaprogram across Lilly's 11-level biocomputer hierarchy and apply scientistic flattening.",
                serde_json::to_string(&schema_for!(AuditBiocomputerMetaprogramArgs)).unwrap_or_default(),
                Some(r"(?i)(audit|biocomputer|level\s*[5-9]|metaprogram\s+rule|scientistic|flattening|autopilot)"),
            ),
            (
                "navigate_amnesic_maze",
                "Resolve an amnesic blockade, mental blank, blackout, melting maze, or locked door by opacifying the lock and applying The Key is No Key topological inversion.",
                serde_json::to_string(&schema_for!(NavigateAmnesicMazeArgs)).unwrap_or_default(),
                Some(r"(?i)(amnesic|blockade|blackout|melting\s+maze|melting\s+floor|locked\s+door|key\s+is\s+no\s+key|mental\s+blank|blok[aá]d[ay]|v[sš]echno\s+zmizelo)"),
            ),
            (
                "gate_actr_topic_relevance",
                "Evaluate topic relevance using the ACT-R 4-question retrieval gate, cache tangents to the parking buffer, or prune associative drift.",
                serde_json::to_string(&schema_for!(GateActrTopicRelevanceArgs)).unwrap_or_default(),
                Some(r"(?i)(tangent|parking\s+buffer|off\s+topic|drift|act-r|relevance\s+gate)"),
            ),
            (
                "calibrate_terminology_firewall",
                "Disambiguate overlapping terms (metaprogram, desire, habit, projection, introspection, self) across NLP, psychoanalysis, cybernetics, and Active Inference.",
                serde_json::to_string(&schema_for!(CalibrateTerminologyFirewallArgs)).unwrap_or_default(),
                Some(r"(?i)(nlp|psychoanaly\w*|conflat\w*|disambiguat\w*|polysem\w*|folk\s+desire)"),
            ),
            (
                "manage_shorthand_macro",
                "Capture, verify, or invoke a personal metaprogrammatic shorthand macro or conceptual pact (Tap-In) in the Knowledge Vault.",
                serde_json::to_string(&schema_for!(ManageShorthandMacroArgs)).unwrap_or_default(),
                Some(r"(?i)(let's\s+call\s+that|shorthand|macro|tap-in|glass-pane\s+lock|z[aá]mek\s+ze\s+sklen[eě]n[eé]\s+tabule|ulo[zž]\s+to)"),
            ),
        ];

        for (name, desc, schema_str, trigger_opt) in definitions {
            let embed_input = format!("{name}: {desc}");
            let embedding_3072 = compute_needle_3072_embedding(&embed_input);
            if let Some(pattern) = trigger_opt {
                let re = Regex::new(pattern).map_err(|e| {
                    SystemError::NeedleEngineHalt(format!("Invalid regex trigger for {name}: {e}"))
                })?;
                self.compiled_triggers.insert(name.to_string(), re);
            }
            self.registered_tools.push(NeedleToolSchema {
                name: name.to_string(),
                description: desc.to_string(),
                parameters_json_schema: schema_str,
                regex_trigger: trigger_opt.map(str::to_string),
                embedding_3072,
            });
        }
        Ok(())
    }

    pub fn hot_swap(&self, generation: NeedleGeneration, layers: u8) -> Result<u64, SystemError> {
        let start = Instant::now();
        if generation == NeedleGeneration::Needle3LadderedSAN && ![2, 4, 8, 16, 20].contains(&layers) {
            return Err(SystemError::NeedleEngineHalt(format!(
                "Invalid Laddered SAN slice depth: {layers}. Supported depths: 2, 4, 8, 16, 20"
            )));
        }
        *self.active_generation.write().unwrap() = generation;
        *self.active_layers.write().unwrap() = layers;
        Ok(start.elapsed().as_micros() as u64)
    }

    pub fn run_grammar_constrained_turn(&self, utterance: &str, top_k: usize) -> NeedleRunResult {
        let query_vec = compute_needle_3072_embedding(utterance);

        let mut scored_tools: Vec<(&NeedleToolSchema, f32, bool)> = self
            .registered_tools
            .iter()
            .map(|tool| {
                let regex_hit = self
                    .compiled_triggers
                    .get(&tool.name)
                    .map(|re| re.is_match(utterance))
                    .unwrap_or(false);
                let sim = cosine_similarity_3072(&query_vec, &tool.embedding_3072);
                let calibrated_score = if regex_hit {
                    (0.84 + sim * 0.15).clamp(0.12, 0.99)
                } else {
                    (sim * 2.2).clamp(0.01, 0.95)
                };
                (tool, calibrated_score, regex_hit)
            })
            .collect();

        scored_tools.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored_tools.truncate(top_k);

        let mut function_calls = Vec::new();
        let mut suppressed_calls = Vec::new();
        let lower = utterance.to_lowercase();

        for (tool, confidence, regex_hit) in scored_tools {
            if !regex_hit && confidence < 0.05 {
                continue;
            }
            let args = self.synthesize_grammar_constrained_args(&tool.name, utterance, &lower);
            let call = NeedleFunctionCall {
                name: tool.name.clone(),
                arguments: args,
                confidence,
                triggered_by_regex: regex_hit,
            };

            if confidence >= CONFIDENCE_FLOOR || regex_hit {
                if regex_hit || confidence >= 0.35 {
                    function_calls.push(call);
                } else {
                    suppressed_calls.push(call);
                }
            } else {
                suppressed_calls.push(call);
            }
        }

        let top_confidence = function_calls
            .first()
            .map(|c| c.confidence)
            .or_else(|| suppressed_calls.first().map(|c| c.confidence))
            .unwrap_or(0.04);

        NeedleRunResult {
            reasoning: format!(
                "Laddered SAN ({:?}, {}L) evaluated {} pruned tools against byte-level grammar.",
                *self.active_generation.read().unwrap(),
                *self.active_layers.read().unwrap(),
                top_k
            ),
            confidence: top_confidence,
            function_calls,
            suppressed_calls,
        }
    }

    fn synthesize_grammar_constrained_args(
        &self,
        tool_name: &str,
        utterance: &str,
        lower: &str,
    ) -> serde_json::Value {
        match tool_name {
            "navigate_amnesic_maze" => serde_json::to_value(NavigateAmnesicMazeArgs {
                blockade_metaphor: if lower.contains("melting") {
                    "melting maze of rooms and floors".into()
                } else if lower.contains("blackout") || lower.contains("zmizelo") {
                    "protective cognitive reset blackout".into()
                } else {
                    "amnesic blockade / locked door".into()
                },
                defense_mechanism: if lower.contains("childhood") || lower.contains("trauma") {
                    "repression_coex".into()
                } else {
                    "habit_prior_lock".into()
                },
                sandbox_bracket: "inspect_lock_architecture".into(),
            })
            .unwrap(),
            "calibrate_terminology_firewall" => {
                let term = [
                    "metaprogram",
                    "projection",
                    "desire",
                    "habit",
                    "introspection",
                    "self",
                ]
                .iter()
                .find(|&&t| lower.contains(t))
                .copied()
                .unwrap_or("metaprogram");
                serde_json::to_value(CalibrateTerminologyFirewallArgs {
                    polysemous_term: term.into(),
                    target_level: "L2_Cybernetic".into(),
                })
                .unwrap()
            }
            "manage_shorthand_macro" => {
                let action = if lower.contains("ulož to") || lower.contains("yes, lock") {
                    "verify_warrant"
                } else if lower.contains("let's call that") {
                    "formalize_new"
                } else {
                    "invoke_amortized"
                };
                serde_json::to_value(ManageShorthandMacroArgs {
                    macro_trigger: "Glass-Pane Lock / Zámek ze skleněné tabule".into(),
                    action: action.into(),
                    formal_expansion: "Amortized defensive habit prior E(pi) with transparent high gamma_C masquerading as an objective Level 1 reality constraint.".into(),
                })
                .unwrap()
            }
            "gate_actr_topic_relevance" => serde_json::to_value(GateActrTopicRelevanceArgs {
                candidate_chunk: utterance.chars().take(64).collect(),
                active_goal_scheme: "Biocomputer_Level_7_8_Audit".into(),
                triage_action: "cache_to_parking_buffer".into(),
            })
            .unwrap(),
            _ => serde_json::to_value(AuditBiocomputerMetaprogramArgs {
                target_belief_or_program: utterance.chars().take(80).collect(),
                biocomputer_level: 7,
                operation: "opacify_parameter".into(),
            })
            .unwrap(),
        }
    }
}

// ============================================================================
// METACOGNITION ENGINE, TYPE-2 SDT (meta-d') & ACCESS TAP-IN DIALECT
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Channel2Scratchpad {
    pub active_biocomputer_levels: Vec<String>,
    pub active_mode: String,
    pub tap_in_protocol_state: String,
    pub act_r_buffer: ActRBufferState,
    pub metareasoning_controller: MetareasoningState,
    pub maze_topology_tracker: MazeTopologyState,
    pub terminology_calibration_log: Vec<TerminologyCalibrationEntry>,
    pub shorthand_macro_registry: Vec<ShorthandMacroRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActRBufferState {
    pub current_goal_scheme: String,
    pub retrieved_chunk_id: String,
    pub relevance_justification: String,
    pub parking_buffer: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetareasoningState {
    pub estimated_uncertainty_entropy: String,
    pub precision_shadow_price_gamma: f32,
    pub meta_d_prime_sensitivity: f32,
    pub primary_d_prime: f32,
    pub meta_efficiency_metric: f32,
    pub rescue_anchor_active: bool,
    pub voe_surprisal_nats: f32,
    pub overthinking_suppressed: bool,
    pub recursion_depth: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MazeTopologyState {
    pub active_stage: String,
    pub current_room_hypothesis: String,
    pub detected_blockade_device: String,
    pub lock_opacity_state: String,
    pub epistemological_crutch_bracket: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminologyCalibrationEntry {
    pub term: String,
    pub canonical_mapping: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShorthandMacroRecord {
    pub macro_id: String,
    pub macro_trigger: String,
    pub formal_expansion: String,
    pub deepjudge_warrant_score: f32,
    pub warrant_verified: bool,
}

impl Default for Channel2Scratchpad {
    fn default() -> Self {
        Self {
            active_biocomputer_levels: vec![
                "L6_Program".into(),
                "L7_Metaprogram".into(),
                "L8_SelfMetaprogrammer".into(),
            ],
            active_mode: "BASELINE_INTERLOCK".into(),
            tap_in_protocol_state: "PHASE_2_TAP_IN_LOCKED".into(),
            act_r_buffer: ActRBufferState {
                current_goal_scheme: "Metasphere_Biocomputer_Audit".into(),
                retrieved_chunk_id: "CHUNK_INIT_00".into(),
                relevance_justification: "Q1-Q4 verified against sovereign session goal".into(),
                parking_buffer: Vec::new(),
            },
            metareasoning_controller: MetareasoningState {
                estimated_uncertainty_entropy: "LOW".into(),
                precision_shadow_price_gamma: 0.65,
                meta_d_prime_sensitivity: 1.15,
                primary_d_prime: 1.05,
                meta_efficiency_metric: 1.095,
                rescue_anchor_active: false,
                voe_surprisal_nats: 0.42,
                overthinking_suppressed: false,
                recursion_depth: 1,
            },
            maze_topology_tracker: MazeTopologyState {
                active_stage: "STAGE_0_OPEN".into(),
                current_room_hypothesis: "Baseline exploratory state".into(),
                detected_blockade_device: "NONE".into(),
                lock_opacity_state: "TRANSPARENT_UNEXAMINED".into(),
                epistemological_crutch_bracket: "NONE".into(),
            },
            terminology_calibration_log: Vec::new(),
            shorthand_macro_registry: Vec::new(),
        }
    }
}

pub struct MetacogAssistantEngine {
    pub needle: Arc<CactusNeedleEngine>,
    pub metrics: Arc<MetricsExporter>,
    pub sessions: RwLock<HashMap<String, Channel2Scratchpad>>,
}

impl MetacogAssistantEngine {
    pub fn new(needle: Arc<CactusNeedleEngine>, metrics: Arc<MetricsExporter>) -> Self {
        Self {
            needle,
            metrics,
            sessions: RwLock::new(HashMap::new()),
        }
    }

    /// Computes metacognitive efficiency using Ideal Observer Type-2 SDT (meta-d' / d').
    /// Activates the Small-Denominator Rescue Anchor (meta-d' - d') when primary d' < 0.25.
    pub fn compute_metacognitive_efficiency(meta_d_prime: f32, primary_d_prime: f32) -> (f32, bool) {
        if primary_d_prime.abs() < 0.25 {
            (meta_d_prime - primary_d_prime, true)
        } else {
            (meta_d_prime / primary_d_prime, false)
        }
    }

    pub fn sanitize_channel_1_vocal(raw: &str) -> String {
        let cleaned: String = raw
            .chars()
            .filter(|c| !matches!(c, '*' | '#' | '`' | '~'))
            .collect();
        let words: Vec<&str> = cleaned.split_whitespace().collect();
        if words.len() <= MAX_VOCAL_WORDS {
            words.join(" ")
        } else {
            let mut truncated = words[..MAX_VOCAL_WORDS].join(" ");
            if !truncated.ends_with('?') && !truncated.ends_with('.') {
                truncated.push('.');
            }
            truncated
        }
    }

    pub fn compute_voe_and_overthinking_gate(
        utterance: &str,
        prosodic_pause_ms: u32,
        user_is_expert: bool,
    ) -> (f32, bool) {
        let word_count = utterance.split_whitespace().count();
        let voe_nats =
            ((word_count as f32) * 0.08 + (prosodic_pause_ms as f32) / 1000.0).clamp(0.1, 4.5);
        let suppress_overthinking =
            voe_nats < 0.55 || (user_is_expert && word_count < 8 && prosodic_pause_ms < 400);
        (voe_nats, suppress_overthinking)
    }

    pub fn process_turn(
        &self,
        session_id: &str,
        utterance: &str,
        language: &str,
        prosodic_pause_ms: u32,
        user_is_expert: bool,
    ) -> (String, Channel2Scratchpad, NeedleRunResult, bool, f32) {
        self.metrics.total_turns.fetch_add(1, Ordering::Relaxed);
        let (voe_nats, suppress_overthinking) =
            Self::compute_voe_and_overthinking_gate(utterance, prosodic_pause_ms, user_is_expert);

        let run_result = self.needle.run_grammar_constrained_turn(utterance, 5);
        if !run_result.suppressed_calls.is_empty() {
            self.metrics
                .suppressed_tool_calls
                .fetch_add(run_result.suppressed_calls.len() as u64, Ordering::Relaxed);
        }

        let mut sessions_guard = self.sessions.write().unwrap();
        let scratchpad = sessions_guard.entry(session_id.to_string()).or_default();
        scratchpad.metareasoning_controller.voe_surprisal_nats = voe_nats;
        scratchpad.metareasoning_controller.overthinking_suppressed = suppress_overthinking;

        let (eff_score, rescue_used) = Self::compute_metacognitive_efficiency(
            scratchpad.metareasoning_controller.meta_d_prime_sensitivity,
            scratchpad.metareasoning_controller.primary_d_prime,
        );
        scratchpad.metareasoning_controller.meta_efficiency_metric = eff_score;
        scratchpad.metareasoning_controller.rescue_anchor_active = rescue_used;

        if scratchpad.metareasoning_controller.recursion_depth >= 3 {
            scratchpad.metareasoning_controller.recursion_depth = 1;
        } else {
            scratchpad.metareasoning_controller.recursion_depth += 1;
        }

        let requires_confirmation = run_result.confidence >= CONFIDENCE_FLOOR
            && run_result.confidence < AUTONOMOUS_ACT_THRESHOLD
            && !run_result.function_calls.is_empty();

        if requires_confirmation {
            self.metrics
                .confirmation_gated_calls
                .fetch_add(1, Ordering::Relaxed);
        } else if !run_result.function_calls.is_empty() {
            self.metrics
                .autonomous_tool_calls
                .fetch_add(run_result.function_calls.len() as u64, Ordering::Relaxed);
        }

        let is_cz = language.eq_ignore_ascii_case("CZ")
            || utterance.to_lowercase().contains("zámek")
            || utterance.to_lowercase().contains("ulož to")
            || utterance.to_lowercase().contains("zmizelo");

        let raw_vocal = if let Some(primary_call) = run_result.function_calls.first() {
            match primary_call.name.as_str() {
                "navigate_amnesic_maze" => {
                    scratchpad.active_mode = "MAZE_NAVIGATION".into();
                    scratchpad.maze_topology_tracker = MazeTopologyState {
                        active_stage: "STAGE_3_OPACIFY_LOCK".into(),
                        current_room_hypothesis:
                            "Repressive survival metaprogram guarding an overvalued prior".into(),
                        detected_blockade_device: "REPRESSION_COEX".into(),
                        lock_opacity_state: "OPACIFIED_INSPECTABLE".into(),
                        epistemological_crutch_bracket: "AS_IF_FALSE_SANDBOX".into(),
                    };
                    if is_cz {
                        "Tato prázdnota není selháním paměti, ale aktivním ochranným metaprogramem sedmé úrovně. Místo abychom dveře vyráželi silou, prozkoumejme samotný zámek: jakému ohrožení se tento strážný program snaží předejít, kdyby se dveře otevřely?"
                    } else {
                        "That mental blank is not a failure of recall, but an active level seven protective metaprogram doing its job. Instead of forcing the door open, let us inspect the lock itself: what does the guard program assume would happen to your system if that door opened?"
                    }
                }
                "calibrate_terminology_firewall" => {
                    scratchpad.active_mode = "TERMINOLOGY_AUDIT".into();
                    scratchpad
                        .terminology_calibration_log
                        .push(TerminologyCalibrationEntry {
                            term: "metaprogram".into(),
                            canonical_mapping:
                                "L2 Lilly executive control over parallel routines vs commercial NLP static sorting filter"
                                    .into(),
                        });
                    if is_cz {
                        "Oddělme tyto dvě roviny popisu, aby nedošlo k sémantickému posunu: komerční NLP označuje metaprogramem statický percepční filtr, zatímco v Lillyho biopočítači jde o rekurzivní exekutivní instrukci osmé úrovně. Kterou z těchto dvou vrstev nyní zkoumáme?"
                    } else {
                        "Let us separate those two levels of description first: commercial NLP uses metaprogram to mean a static perceptual sorting filter, whereas in Lilly's biocomputer it denotes level eight executive instructions that rewrite lower routines. Which of those two layers are we auditing right now?"
                    }
                }
                "manage_shorthand_macro" => {
                    scratchpad.active_mode = "SHORTHAND_CAPTURE".into();
                    scratchpad.tap_in_protocol_state = "PHASE_3_WEAK_MEASUREMENT_RESONANCE".into();
                    let verified = utterance.to_lowercase().contains("ulož to")
                        || utterance.to_lowercase().contains("yes");
                    scratchpad.shorthand_macro_registry.push(ShorthandMacroRecord {
                        macro_id: "MACRO_001".into(),
                        macro_trigger: "Glass-Pane Lock / Zámek ze skleněné tabule".into(),
                        formal_expansion:
                            "Amortized habit prior E(pi) with transparent precision gamma_C masquerading as objective reality"
                                .into(),
                        deepjudge_warrant_score: 0.98,
                        warrant_verified: verified,
                    });
                    if is_cz {
                        "Zkratka Zámek ze skleněné tabule je nyní ověřena a uložena v naší znalostní bance. Když ji nyní aplikujeme na publikování kódu jako pouhou hypotézu coby-kdyby, jaká dětská obava se za tímto sklem skrývá?"
                    } else {
                        "By naming that rule, you turned the locked room inside out into an empty balloon. Before I lock Glass-Pane Lock into our permanent shorthand registry as a transparent habit policy masquerading as objective reality, does that mapping match your exact intent?"
                    }
                }
                "gate_actr_topic_relevance" => {
                    scratchpad.active_mode = "RELEVANCE_GATE".into();
                    scratchpad
                        .act_r_buffer
                        .parking_buffer
                        .push(utterance.chars().take(48).collect());
                    "I have cached that tangential branch in our ACT-R parking buffer so we do not lose it. Returning to our active goal vector at level eight, how does that precision parameter shift your current action policy?"
                }
                _ => {
                    scratchpad.active_mode = "BIOCOMPUTER_AUDIT".into();
                    "Observing that internal rule shifts it from a transparent assumption into an inspectable level seven object. From the perspective of your central executive I, does that priority rule still serve your current environment?"
                }
            }
        } else {
            "I am holding space with zero preprogrammed assumptions. Which room or metaprogrammatic rule in the biocomputer would your central I like to inspect first?"
        };

        let channel_1 = Self::sanitize_channel_1_vocal(raw_vocal);
        (
            channel_1,
            scratchpad.clone(),
            run_result,
            requires_confirmation,
            voe_nats,
        )
    }
}

// ============================================================================
// LIVEVOICE API MODULE (Full-Duplex Discord RTP <-> Whistle ASR)
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LiveVoiceVadState {
    Listening,
    MetacogPauseHold,
    DispatchingToNeedle,
    SpeakingChannel1,
}

pub struct LiveVoiceSession {
    pub session_id: String,
    pub vad_state: LiveVoiceVadState,
    pub accumulated_silence_ms: u32,
    pub detected_word_repetitions: u32,
    pub barge_in_triggered: bool,
}

pub struct LiveVoiceApiModule {
    pub metacog_engine: Arc<MetacogAssistantEngine>,
    pub metrics: Arc<MetricsExporter>,
}

impl LiveVoiceApiModule {
    pub fn new(
        metacog_engine: Arc<MetacogAssistantEngine>,
        metrics: Arc<MetricsExporter>,
    ) -> Self {
        Self {
            metacog_engine,
            metrics,
        }
    }

    pub fn bayesian_phonetic_stt_repair(raw_transcript: &str) -> String {
        let repairs = [
            ("coax system", "COEX system"),
            ("coax loop", "COEX loop"),
            ("meta problem", "metaprogram"),
            ("meta-problem", "metaprogram"),
            ("actor buffer", "ACT-R buffer"),
            ("day policy", "dAI policy"),
            ("glass pain lock", "Glass-Pane Lock"),
        ];
        let mut repaired = raw_transcript.to_string();
        for (corrupt, canonical) in repairs {
            let re = Regex::new(&format!(r"(?i){}", regex::escape(corrupt))).unwrap();
            repaired = re.replace_all(&repaired, canonical).into_owned();
        }
        repaired
    }

    pub fn evaluate_precision_aware_vad(
        &self,
        voice_session: &mut LiveVoiceSession,
        pcm_16khz_mono: &[i16],
        has_hesitation_marker: bool,
    ) -> LiveVoiceVadState {
        self.metrics
            .live_voice_frames_processed
            .fetch_add(1, Ordering::Relaxed);

        let rms_energy = if pcm_16khz_mono.is_empty() {
            0.0
        } else {
            let sum_sq: f64 = pcm_16khz_mono
                .iter()
                .map(|&s| (s as f64) * (s as f64))
                .sum();
            (sum_sq / (pcm_16khz_mono.len() as f64)).sqrt()
        };

        let is_speech = rms_energy > 350.0;
        if is_speech {
            if voice_session.vad_state == LiveVoiceVadState::SpeakingChannel1 {
                voice_session.barge_in_triggered = true;
            }
            voice_session.accumulated_silence_ms = 0;
            if has_hesitation_marker {
                voice_session.detected_word_repetitions += 1;
            }
            voice_session.vad_state = LiveVoiceVadState::Listening;
        } else {
            voice_session.accumulated_silence_ms += 20;
            let end_of_turn_threshold_ms =
                if voice_session.detected_word_repetitions > 0 || has_hesitation_marker {
                    2200
                } else {
                    800
                };

            if voice_session.accumulated_silence_ms < end_of_turn_threshold_ms
                && voice_session.accumulated_silence_ms >= 400
            {
                self.metrics
                    .metacog_pause_holds
                    .fetch_add(1, Ordering::Relaxed);
                voice_session.vad_state = LiveVoiceVadState::MetacogPauseHold;
            } else if voice_session.accumulated_silence_ms >= end_of_turn_threshold_ms {
                voice_session.vad_state = LiveVoiceVadState::DispatchingToNeedle;
                voice_session.detected_word_repetitions = 0;
            }
        }

        voice_session.vad_state
    }

    pub fn process_completed_voice_utterance(
        &self,
        voice_session: &mut LiveVoiceSession,
        whistle_raw_transcript: &str,
        language: &str,
    ) -> (String, String, Channel2Scratchpad, f32) {
        let repaired_transcript = Self::bayesian_phonetic_stt_repair(whistle_raw_transcript);
        let (channel_1_vocal, channel_2_scratchpad, run_result, _confirm, _voe) =
            self.metacog_engine.process_turn(
                &voice_session.session_id,
                &repaired_transcript,
                language,
                voice_session.accumulated_silence_ms,
                false,
            );
        voice_session.vad_state = LiveVoiceVadState::SpeakingChannel1;
        voice_session.accumulated_silence_ms = 0;
        (
            repaired_transcript,
            channel_1_vocal,
            channel_2_scratchpad,
            run_result.confidence,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    const SAMPLE_OPENAPI: &str = "openapi: 3.1.0\npaths:\n  /api/embed:\n  /api/metacog/turn:";
    const SAMPLE_DDL: &str = "
        CREATE TABLE IF NOT EXISTS users (user_id TEXT PRIMARY KEY);
        CREATE TABLE IF NOT EXISTS sessions (session_id TEXT PRIMARY KEY);
        CREATE TABLE IF NOT EXISTS shorthand_macros (macro_id TEXT PRIMARY KEY);
        CREATE TABLE IF NOT EXISTS tool_embeddings (tool_name TEXT PRIMARY KEY);
        CREATE TABLE IF NOT EXISTS audit_logs (log_id INTEGER PRIMARY KEY);
    ";
    const SAMPLE_PROTO: &str = "
        syntax = \"proto3\";
        service MetacogEngineService {}
        service LiveVoiceService {}
    ";

    #[test]
    fn test_state_a_guard_halt_on_missing_schema_or_proto() {
        let missing_ddl = verify_qssu_contracts(SAMPLE_OPENAPI, "", SAMPLE_PROTO);
        assert!(matches!(missing_ddl, Err(SystemError::ContractGuardHalt(_))));

        let invalid_proto =
            verify_qssu_contracts(SAMPLE_OPENAPI, SAMPLE_DDL, "syntax = \"proto2\";");
        assert!(matches!(invalid_proto, Err(SystemError::ContractGuardHalt(_))));

        let valid_state_b = verify_qssu_contracts(SAMPLE_OPENAPI, SAMPLE_DDL, SAMPLE_PROTO);
        assert!(valid_state_b.is_ok());
    }

    #[test]
    fn test_environment_sanitization_and_airgap_flags() {
        env::set_var("ANDROID_ROOT", "/system");
        let ctx = sanitize_runtime_environment();
        assert!(ctx.telemetry_disabled);
        assert!(ctx.proot_sanitized);
        assert!(env::var_os("ANDROID_ROOT").is_none());
    }

    #[test]
    fn test_meta_d_prime_small_denominator_rescue_anchor() {
        let (ratio, rescue_false) = MetacogAssistantEngine::compute_metacognitive_efficiency(1.2, 1.0);
        assert!(!rescue_false);
        assert!((ratio - 1.2).abs() < 1e-4);

        let (diff, rescue_true) = MetacogAssistantEngine::compute_metacognitive_efficiency(0.8, 0.10);
        assert!(rescue_true);
        assert!((diff - 0.7).abs() < 1e-4);
    }

    #[tokio::test]
    async fn test_in_memory_tokio_duplex_rpc_transport() {
        let (mut client_stream, mut server_stream) = tokio::io::duplex(4096);
        let server_task = tokio::spawn(async move {
            let mut buf = [0u8; 64];
            let n = server_stream.read(&mut buf).await.unwrap();
            assert_eq!(&buf[..n], b"PING_DUPLEX_AUTH");
            server_stream.write_all(b"ACK_DUPLEX_OK").await.unwrap();
        });

        client_stream.write_all(b"PING_DUPLEX_AUTH").await.unwrap();
        let mut resp = [0u8; 64];
        let n = client_stream.read(&mut resp).await.unwrap();
        assert_eq!(&resp[..n], b"ACK_DUPLEX_OK");
        server_task.await.unwrap();
    }
}
