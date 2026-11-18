//! Login with session cookie. Sessions live in memory (a restart logs everyone out).

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use axum::extract::{Form, Request};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Redirect, Response};
use serde::Deserialize;

use super::{Secret, config};

pub const COOKIE: &str = "nrzdash_session";
const SESSION_TTL: Duration = Duration::from_secs(60 * 60 * 24 * 30);
const MAX_FAILS: usize = 5;
const FAIL_WINDOW: Duration = Duration::from_secs(60);

struct State {
    /// token → expiry (unix seconds). Persisted so restarts don't log everyone out.
    sessions: HashMap<String, u64>,
    fails: Vec<Instant>,
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn sessions_file() -> std::path::PathBuf {
    config().data_dir.join("sessions.json")
}

fn load_sessions() -> HashMap<String, u64> {
    let t = now();
    std::fs::read_to_string(sessions_file())
        .ok()
        .and_then(|s| serde_json::from_str::<HashMap<String, u64>>(&s).ok())
        .map(|m| m.into_iter().filter(|(_, exp)| *exp > t).collect())
        .unwrap_or_default()
}

fn save_sessions(sessions: &HashMap<String, u64>) {
    let path = sessions_file();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(json) = serde_json::to_string(sessions) {
        if std::fs::write(&path, json).is_ok() {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
            }
        }
    }
}

fn state() -> &'static Mutex<State> {
    static S: OnceLock<Mutex<State>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(State { sessions: load_sessions(), fails: Vec::new() }))
}

pub fn random_token(bytes: usize) -> String {
    use rand::RngCore;
    let mut buf = vec![0u8; bytes];
    rand::rngs::OsRng.fill_bytes(&mut buf);
    buf.iter().map(|b| format!("{b:02x}")).collect()
}

fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn verify(user: &str, password: &str) -> bool {
    let cfg = config();
    let user_ok = ct_eq(user.as_bytes(), cfg.user.as_bytes());
    let pass_ok = match &cfg.password {
        Secret::Plain(p) => ct_eq(password.as_bytes(), p.as_bytes()),
        Secret::Hash(h) => {
            use argon2::{Argon2, PasswordHash, PasswordVerifier};
            PasswordHash::new(h)
                .is_ok_and(|parsed| Argon2::default().verify_password(password.as_bytes(), &parsed).is_ok())
        }
    };
    user_ok && pass_ok
}

pub fn hash_password(password: &str) -> Result<String, String> {
    use argon2::password_hash::{PasswordHasher, SaltString};
    let salt = SaltString::generate(&mut rand::rngs::OsRng);
    argon2::Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| e.to_string())
}

fn session_from_headers(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|kv| kv.trim().split_once('='))
        .find(|(k, _)| *k == COOKIE)
        .map(|(_, v)| v.to_string())
}

pub fn is_authenticated(headers: &HeaderMap) -> bool {
    let Some(token) = session_from_headers(headers) else { return false };
    let Ok(mut st) = state().lock() else { return false };
    match st.sessions.get(&token) {
        Some(exp) if *exp > now() => true,
        Some(_) => {
            st.sessions.remove(&token);
            save_sessions(&st.sessions);
            false
        }
        None => false,
    }
}

fn cookie(value: &str, max_age: u64) -> HeaderValue {
    let secure = if config().cookie_secure { "; Secure" } else { "" };
    let s = format!("{COOKIE}={value}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age}{secure}");
    HeaderValue::from_str(&s).unwrap_or_else(|_| HeaderValue::from_static(""))
}

#[derive(Deserialize)]
pub struct LoginForm {
    username: String,
    password: String,
}

pub async fn login_handler(Form(form): Form<LoginForm>) -> Response {
    // Global brute-force brake: max. 5 failed attempts per minute.
    let locked = {
        let Ok(mut st) = state().lock() else { return StatusCode::INTERNAL_SERVER_ERROR.into_response() };
        let now = Instant::now();
        st.fails.retain(|t| now.duration_since(*t) < FAIL_WINDOW);
        st.fails.len() >= MAX_FAILS
    };
    if locked {
        return Redirect::to("/login?error=locked").into_response();
    }

    if !verify(form.username.trim(), &form.password) {
        if let Ok(mut st) = state().lock() {
            st.fails.push(Instant::now());
        }
        tokio::time::sleep(Duration::from_millis(800)).await;
        return Redirect::to("/login?error=invalid").into_response();
    }

    let token = random_token(32);
    if let Ok(mut st) = state().lock() {
        let t = now();
        st.sessions.retain(|_, exp| *exp > t);
        st.sessions.insert(token.clone(), t + SESSION_TTL.as_secs());
        save_sessions(&st.sessions);
    }
    let mut res = Redirect::to("/").into_response();
    res.headers_mut().append(header::SET_COOKIE, cookie(&token, SESSION_TTL.as_secs()));
    res
}

pub async fn logout_handler(headers: HeaderMap) -> Response {
    if let Some(token) = session_from_headers(&headers) {
        if let Ok(mut st) = state().lock() {
            st.sessions.remove(&token);
            save_sessions(&st.sessions);
        }
    }
    let mut res = Redirect::to("/login").into_response();
    res.headers_mut().append(header::SET_COOKIE, cookie("", 0));
    res
}

fn is_public(path: &str) -> bool {
    path == "/login"
        || path == "/auth/login"
        || path.starts_with("/assets/")
        || path.starts_with("/wasm/")
        || path.starts_with("/_dioxus")
        || path == "/favicon.ico"
        || path == "/manifest.json"
        || path.starts_with("/monitors/push/")
}

/// Everything except the login page and static assets needs a valid session.
pub async fn auth_middleware(req: Request, next: Next) -> Response {
    let path = req.uri().path();
    if is_public(path) || is_authenticated(req.headers()) {
        return next.run(req).await;
    }
    if path.starts_with("/api/") || path.starts_with("/files/") || path.starts_with("/auth/") {
        return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
    }
    Redirect::to("/login").into_response()
}
