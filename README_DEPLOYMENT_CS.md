# Effortless 0-Cost Deployment Balíček: Cactus Needle Metacognition & LiveVoice (Rust)

## 1. Struktura balíčku (Zero-Mockup, 100% kompletní)
- `Cargo.toml` & `build.rs`: Konfigurace Rustu s agresivní optimalizací velikosti (`opt-level = "z"`, `lto = "fat"`, `strip = "symbols"`).
- `contracts/openapi.yaml` & `contracts/schema.sql`: Striktní kontrakty pro `IngestSpec` (OpenAPI 3.1 + SQLite WAL 3 072-dim vektorová databáze).
- `proto/needle_discord_metacog.proto`: gRPC kontrakt (`AuthService`, `MetacogEngineService`, `LiveVoiceService`).
- `sqlx-data.json`: Offline metadata dotazů pro kompilaci s `SQLX_OFFLINE=true` bez potřeby běžící databáze.
- `src/lib.rs`, `src/discord_bot.rs`, `src/main.rs`: Kompletní implementace (`AuthEngine`, `TokenManager` se `secrecy` + `zeroize`, `DBPool`, `gRPCServer` s `tokio::io::duplex`, `MetricsExporter`, `CactusNeedleEngine`, `LiveVoiceApiModule`, `meta-d'` metrika se záchrannou kotvou `meta-d' - d'` a protokol `Access Tap-In`).
- `Dockerfile` & `docker-compose.yml`: Vícefázový Distroless kontejner (~18 MB binárka + kontrakty pouze pro čtení).
- `leapcell.yaml` & `fly.toml`: Konfigurace pro okamžité bezplatné nasazení.
- `scripts/deploy_termux_proot.sh` & `scripts/verify_and_package.sh`: Jednopříkazové skripty pro lokální mobilní (Android Termux) i serverové spuštění.

## 2. Rychlé spuštění (1 příkaz)
### A) Lokálně / Android Termux (100% 0-Cost Offline)
```bash
chmod +x scripts/deploy_termux_proot.sh && ./scripts/deploy_termux_proot.sh
```

### B) Docker / Distroless Sandbox
```bash
docker compose up --build -d
```

### C) Leapcell (0-Cost Cloud PaaS pro Rust)
1. Nahrajte repozitář na GitHub.
2. Připojte repozitář k Leapcell (automaticky načte `leapcell.yaml`).
3. Nastavte `DISCORD_BOT_TOKEN` v proměnných prostředí (SecretString zajistí automatické maskování `[[REDACTED]]` v logu).
