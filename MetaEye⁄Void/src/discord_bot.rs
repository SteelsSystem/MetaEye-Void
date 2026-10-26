use std::sync::Arc;
use crate::{
    AuthEngine, LiveVoiceApiModule, MetacogAssistantEngine, NeedleGeneration, SovereignRole,
};

pub struct DiscordBotContext {
    pub metacog: Arc<MetacogAssistantEngine>,
    pub live_voice: Arc<LiveVoiceApiModule>,
}

impl DiscordBotContext {
    pub fn handle_discord_turn(
        &self,
        discord_user_id: &str,
        channel_id: &str,
        utterance: &str,
        language: &str,
        explicit_confirmation: bool,
    ) -> Result<(String, String, bool), String> {
        let session_id = format!("discord-{channel_id}-{discord_user_id}");
        let (channel_1, channel_2, run_result, requires_confirmation, _voe) =
            self.metacog
                .process_turn(&session_id, utterance, language, 0, false);

        let is_rewrite = run_result
            .function_calls
            .iter()
            .any(|c| c.name == "manage_shorthand_macro");
        if is_rewrite && !explicit_confirmation && requires_confirmation {
            AuthEngine::evaluate_access(SovereignRole::SovereignI, true, false)
                .map_err(|e| e.to_string())?;
        }

        let sidecar_json = serde_json::to_string_pretty(&channel_2).unwrap_or_default();
        Ok((channel_1, sidecar_json, requires_confirmation))
    }

    pub fn handle_model_hotswap(&self, generation: u8, layers: u8) -> Result<String, String> {
        let gen_enum = match generation {
            2 => NeedleGeneration::Needle2Legacy14MB,
            3 => NeedleGeneration::Needle3LadderedSAN,
            _ => {
                return Err(
                    "Unsupported generation: choose 2 (Needle 2 14MB) or 3 (Needle 3 SAN)".into(),
                )
            }
        };
        let latency_us = self
            .metacog
            .needle
            .hot_swap(gen_enum, layers)
            .map_err(|e| e.to_string())?;
        Ok(format!(
            "Hot-swapped active Cactus engine to {:?} ({}L) in {} us.",
            gen_enum, layers, latency_us
        ))
    }
}
