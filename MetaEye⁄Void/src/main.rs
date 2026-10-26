use std::env;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use needle_discord_metacog::discord_bot::DiscordBotContext;
use needle_discord_metacog::{
    sanitize_runtime_environment, verify_qssu_contracts, CactusNeedleEngine, LiveVoiceApiModule,
    MetacogAssistantEngine, MetricsExporter,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let runtime_ctx = sanitize_runtime_environment();

    let openapi_spec = std::fs::read_to_string("contracts/openapi.yaml")?;
    let database_schema = std::fs::read_to_string("contracts/schema.sql")?;
    let proto_spec = std::fs::read_to_string("proto/needle_discord_metacog.proto")?;

    // QSS-U Contract Guard: halts immediately on missing or malformed contracts (State A)
    verify_qssu_contracts(&openapi_spec, &database_schema, &proto_spec)?;

    let metrics = Arc::new(MetricsExporter::default());
    let needle = Arc::new(CactusNeedleEngine::initialize_default()?);
    let metacog = Arc::new(MetacogAssistantEngine::new(needle, metrics.clone()));
    let live_voice = Arc::new(LiveVoiceApiModule::new(metacog.clone(), metrics.clone()));
    let _discord_ctx = Arc::new(DiscordBotContext {
        metacog: metacog.clone(),
        live_voice,
    });

    let port = env::var("PORT").unwrap_or_else(|_| "8080".to_string());
    let bind_addr = format!("0.0.0.0:{port}");
    let listener = TcpListener::bind(&bind_addr).await?;

    println!(
        "{{\"status\":\"STATE_B_ACTIVE\",\"bind\":\"{bind_addr}\",\"telemetry_disabled\":{}}}",
        runtime_ctx.telemetry_disabled
    );

    loop {
        let (mut socket, _) = listener.accept().await?;
        let metrics_ref = metrics.clone();
        let metacog_ref = metacog.clone();

        tokio::spawn(async move {
            let mut buf = [0u8; 4096];
            let n = match socket.read(&mut buf).await {
                Ok(n) if n > 0 => n,
                _ => return,
            };
            let req = String::from_utf8_lossy(&buf[..n]);

            let (status, content_type, body) = if req.starts_with("GET /metrics") {
                (
                    "200 OK",
                    "text/plain; version=0.0.4",
                    metrics_ref.render_prometheus(),
                )
            } else if req.starts_with("POST /api/metacog/turn") {
                let (c1, c2, _, confirm, _) = metacog_ref.process_turn(
                    "default-http-session",
                    "Inspect amnesic blockade at the locked door",
                    "EN",
                    0,
                    false,
                );
                let payload = serde_json::json!({
                    "channel_1_vocal": c1,
                    "channel_2_scratchpad": c2,
                    "requires_human_confirmation": confirm
                });
                ("200 OK", "application/json", payload.to_string())
            } else {
                (
                    "200 OK",
                    "application/json",
                    "{\"status\":\"OK\",\"qssu_state\":\"STATE_B\",\"engine\":\"Cactus_Needle_3_CQ2\"}".to_string(),
                )
            };

            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = socket.write_all(response.as_bytes()).await;
        });
    }
}
