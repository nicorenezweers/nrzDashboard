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
/// Short-lived cookie between password and 2FA code.
const PENDING_COOKIE: &str = "nrzdash_pending";
const PENDING_TTL: Duration = Duration::from_secs(300);
const SESSION_TTL: Duration = Duration::from_secs(60 * 60 * 24 * 30);
const MAX_FAILS: usize = 5;
const FAIL_WINDOW: Duration = Duration::from_secs(60);

struct State {
    /// token → expiry (unix seconds). Persisted so restarts don't log everyone out.
    sessions: HashMap<String, u64>,
    fails: Vec<Instant>,
    /// Password checked, 2FA code still missing: token → created.
    pending: HashMap<String, Instant>,
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
    S.get_or_init(|| Mutex::new(State { sessions: load_sessions(), fails: Vec::new(), pending: HashMap::new() }))
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

fn verify_hash(hash: &str, password: &str) -> bool {
    use argon2::{Argon2, PasswordHash, PasswordVerifier};
    PasswordHash::new(hash).is_ok_and(|parsed| Argon2::default().verify_password(password.as_bytes(), &parsed).is_ok())
}

/// Checks the password: the one set in the settings, else the one from the environment.
pub fn verify_password(password: &str) -> bool {
    let own = super::dash::get().password_hash;
    if !own.is_empty() {
        return verify_hash(&own, password);
    }
    match &config().password {
        Secret::Plain(p) => ct_eq(password.as_bytes(), p.as_bytes()),
        Secret::Hash(h) => verify_hash(h, password),
    }
}

fn verify(user: &str, password: &str) -> bool {
    let user_ok = ct_eq(user.as_bytes(), config().user.as_bytes());
    let pass_ok = verify_password(password);
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

fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|kv| kv.trim().split_once('='))
        .find(|(k, _)| *k == name)
        .map(|(_, v)| v.to_string())
}

fn session_from_headers(headers: &HeaderMap) -> Option<String> {
    cookie_value(headers, COOKIE)
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

fn named_cookie(name: &str, value: &str, max_age: u64) -> HeaderValue {
    let secure = if config().cookie_secure { "; Secure" } else { "" };
    let s = format!("{name}={value}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age}{secure}");
    HeaderValue::from_str(&s).unwrap_or_else(|_| HeaderValue::from_static(""))
}

fn cookie(value: &str, max_age: u64) -> HeaderValue {
    named_cookie(COOKIE, value, max_age)
}

/// Global brute-force brake: max. 5 failed attempts per minute.
fn locked() -> bool {
    let Ok(mut st) = state().lock() else { return true };
    let now = Instant::now();
    st.fails.retain(|t| now.duration_since(*t) < FAIL_WINDOW);
    st.fails.len() >= MAX_FAILS
}

async fn fail(to: &str) -> Response {
    if let Ok(mut st) = state().lock() {
        st.fails.push(Instant::now());
    }
    tokio::time::sleep(Duration::from_millis(800)).await;
    Redirect::to(to).into_response()
}

/// Creates a session and sends the user to the home screen.
fn start_session(mut res: Response) -> Response {
    let token = random_token(32);
    if let Ok(mut st) = state().lock() {
        let t = now();
        st.sessions.retain(|_, exp| *exp > t);
        st.sessions.insert(token.clone(), t + SESSION_TTL.as_secs());
        save_sessions(&st.sessions);
    }
    res.headers_mut().append(header::SET_COOKIE, cookie(&token, SESSION_TTL.as_secs()));
    res
}

#[derive(Deserialize)]
pub struct LoginForm {
    username: String,
    password: String,
}

pub async fn login_handler(Form(form): Form<LoginForm>) -> Response {
    if locked() {
        return Redirect::to("/login?error=locked").into_response();
    }
    if !verify(form.username.trim(), &form.password) {
        return fail("/login?error=invalid").await;
    }
    if super::dash::get().totp_enabled {
        // Password correct: now the code from the authenticator app.
        let token = random_token(24);
        if let Ok(mut st) = state().lock() {
            st.pending.retain(|_, t| t.elapsed() < PENDING_TTL);
            st.pending.insert(token.clone(), Instant::now());
        }
        let mut res = Redirect::to("/login?step=code").into_response();
        res.headers_mut().append(header::SET_COOKIE, named_cookie(PENDING_COOKIE, &token, PENDING_TTL.as_secs()));
        return res;
    }
    start_session(Redirect::to("/").into_response())
}

#[derive(Deserialize)]
pub struct CodeForm {
    code: String,
}

/// Second step: 6-digit code from the authenticator app.
pub async fn code_handler(headers: HeaderMap, Form(form): Form<CodeForm>) -> Response {
    if locked() {
        return Redirect::to("/login?step=code&error=locked").into_response();
    }
    let Some(token) = cookie_value(&headers, PENDING_COOKIE) else {
        return Redirect::to("/login?error=expired").into_response();
    };
    let valid_pending = state().lock().is_ok_and(|mut st| {
        st.pending.retain(|_, t| t.elapsed() < PENDING_TTL);
        st.pending.contains_key(&token)
    });
    if !valid_pending {
        return Redirect::to("/login?error=expired").into_response();
    }
    let d = super::dash::get();
    if !super::dash::totp_check(&d.totp_secret, &form.code) {
        return fail("/login?step=code&error=code").await;
    }
    if let Ok(mut st) = state().lock() {
        st.pending.remove(&token);
    }
    let mut res = start_session(Redirect::to("/").into_response());
    res.headers_mut().append(header::SET_COOKIE, named_cookie(PENDING_COOKIE, "", 0));
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
        || path == "/auth/code"
        || path.starts_with("/assets/")
        || path.starts_with("/wasm/")
        || path.starts_with("/_dioxus")
        || path == "/favicon.ico"
        || path == "/manifest.json"
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
