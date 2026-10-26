PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS users (
    user_id TEXT PRIMARY KEY NOT NULL,
    discord_snowflake TEXT UNIQUE NOT NULL,
    password_hash TEXT NOT NULL,
    public_key_ed25519 BLOB NOT NULL,
    role TEXT NOT NULL CHECK (role IN ('SOVEREIGN_I', 'CO_EXPLORER', 'AUDITOR')),
    expertise_level TEXT NOT NULL DEFAULT 'INTERMEDIATE' CHECK (expertise_level IN ('NOVICE', 'INTERMEDIATE', 'EXPERT')),
    created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS sessions (
    session_id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    active_model_gen INTEGER NOT NULL DEFAULT 3 CHECK (active_model_gen IN (2, 3)),
    active_language TEXT NOT NULL DEFAULT 'EN' CHECK (active_language IN ('EN', 'CZ')),
    biocomputer_level INTEGER NOT NULL DEFAULT 8 CHECK (biocomputer_level BETWEEN 1 AND 11),
    maze_stage TEXT NOT NULL DEFAULT 'STAGE_0_BASELINE',
    lock_opacity_state TEXT NOT NULL DEFAULT 'TRANSPARENT_UNEXAMINED',
    precision_gamma REAL NOT NULL DEFAULT 0.50,
    meta_d_prime REAL NOT NULL DEFAULT 1.00,
    primary_d_prime REAL NOT NULL DEFAULT 1.00,
    tap_in_state TEXT NOT NULL DEFAULT 'CALIBRATING_HILBERT',
    channel_2_json TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS refresh_tokens (
    token_id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    token_hash BLOB NOT NULL,
    expires_at INTEGER NOT NULL,
    revoked INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS shorthand_macros (
    macro_id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    macro_trigger TEXT NOT NULL,
    biocomputer_levels TEXT NOT NULL,
    formal_expansion TEXT NOT NULL,
    default_operator TEXT NOT NULL,
    deepjudge_warrant_score REAL NOT NULL CHECK (deepjudge_warrant_score >= 0.0 AND deepjudge_warrant_score <= 1.0),
    warrant_verified INTEGER NOT NULL DEFAULT 0,
    stale_habit_risk_flag INTEGER NOT NULL DEFAULT 0,
    embedding_3072 BLOB NOT NULL,
    created_at INTEGER NOT NULL,
    UNIQUE(user_id, macro_trigger)
);

CREATE TABLE IF NOT EXISTS tool_embeddings (
    tool_name TEXT PRIMARY KEY NOT NULL,
    schema_json TEXT NOT NULL,
    regex_trigger TEXT,
    embedding_3072 BLOB NOT NULL
);

CREATE TABLE IF NOT EXISTS audit_logs (
    log_id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL,
    turn_index INTEGER NOT NULL,
    selected_tools TEXT NOT NULL,
    suppressed_tools TEXT NOT NULL,
    confidence REAL NOT NULL,
    voe_surprisal_nats REAL NOT NULL,
    meta_efficiency_score REAL NOT NULL,
    timestamp INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_sessions_user ON sessions(user_id);
CREATE INDEX IF NOT EXISTS idx_macros_user_trigger ON shorthand_macros(user_id, macro_trigger);
CREATE INDEX IF NOT EXISTS idx_audit_session ON audit_logs(session_id, turn_index);
