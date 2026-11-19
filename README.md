# nrzDashboard

**Deutsch** — Startbildschirm für umbrelOS, komplett in Rust (Dioxus fullstack): App-Raster mit
Domains (Benutzer oder Admin, HTTPS), App Store, Aktivität (CPU, RAM, Speicher), Dateien, KI-Agent,
Anmeldung mit Passwort und Zwei-Faktor-Code.

**English** — umbrelOS home screen written entirely in Rust (Dioxus fullstack): app grid with
domains (user or admin, HTTPS), App Store, activity (CPU, RAM, storage), files, AI agent,
login with password and two-factor code.

## Einstellungen per Umgebung

| Variable | Bedeutung |
|----------|-----------|
| `DASH_USER`, `DASH_PASSWORD` / `DASH_PASSWORD_HASH` | Anmeldung (Passwort später in den Einstellungen änderbar) |
| `UMBREL_URL`, `UMBREL_MCP_TOKEN` | umbrelOS-MCP (auch in den Einstellungen) |
| `NRZDNS_MCP_URL`, `NRZDNS_MCP_TOKEN` | nrzNetwork-MCP für die App-Domains (auch in den Einstellungen) |
| `DATA_DIR` | Speicherort der Einstellungen |
| `COOKIE_SECURE=1` | hinter HTTPS |

Release: `cargo xtask release <version> "Beschreibung"`.
