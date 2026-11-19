//! Server-only code: configuration, login (password + 2FA), umbrelOS MCP client, app domains, file proxy.

pub mod agent;
pub mod apps;
pub mod auth;
pub mod dash;
pub mod files;
pub mod mcp;
pub mod settings;

use std::path::PathBuf;
use std::sync::OnceLock;

pub struct Config {
    pub user: String,
    /// Either an argon2 PHC hash (`$argon2…`) or a plain password.
    pub password: Secret,
    pub cookie_secure: bool,
    pub data_dir: PathBuf,
}

pub enum Secret {
    Hash(String),
    Plain(String),
}

static CONFIG: OnceLock<Config> = OnceLock::new();

pub fn env(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.trim().is_empty())
}

pub fn config() -> &'static Config {
    CONFIG.get_or_init(|| {
        let password = if let Some(h) = env("DASH_PASSWORD_HASH") {
            Secret::Hash(h)
        } else if let Some(p) = env("DASH_PASSWORD") {
            Secret::Plain(p)
        } else {
            let p = auth::random_token(9);
            tracing_warn(&format!(
                "DASH_PASSWORD ist nicht gesetzt – temporäres Passwort für diesen Start: {p}"
            ));
            Secret::Plain(p)
        };
        Config {
            user: env("DASH_USER").unwrap_or_else(|| "admin".into()),
            password,
            cookie_secure: env("COOKIE_SECURE").is_some_and(|v| v == "1" || v == "true"),
            data_dir: PathBuf::from(env("DATA_DIR").unwrap_or_else(|| "data".into())),
        }
    })
}

pub fn tracing_warn(msg: &str) {
    eprintln!("[nrzDashboard] {msg}");
}

pub fn http() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(5))
            .user_agent("nrzDashboard/1.0")
            .build()
            .unwrap_or_default()
    })
}

/// Builds the full axum router: Dioxus app + auth/file routes + auth middleware.
pub fn router(app: fn() -> dioxus::prelude::Element) -> axum::Router {
    use axum::routing::{get, post};

    // Read config once so a generated password is printed at startup.
    let _ = config();

    dioxus::server::router(app)
        .route("/auth/login", post(auth::login_handler))
        .route("/auth/code", post(auth::code_handler))
        .route("/auth/logout", get(auth::logout_handler).post(auth::logout_handler))
        .route("/files/download", get(files::download_handler))
        .route(
            "/files/upload",
            post(files::upload_handler).layer(axum::extract::DefaultBodyLimit::disable()),
        )
        .merge(files::routes())
        .merge(agent::routes())
        .merge(apps::routes())
        .layer(axum::middleware::from_fn(auth::auth_middleware))
}
