//! Streams file downloads/uploads between the browser and the umbrelOS MCP file endpoints.

use axum::body::{Body, Bytes};
use axum::extract::{Multipart, Query};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use serde::Deserialize;

use super::{http, settings};

fn enc(s: &str) -> String {
    urlencoding::encode(s).into_owned()
}

#[derive(Deserialize)]
pub struct DownloadQuery {
    path: String,
    #[serde(default)]
    inline: Option<String>,
}

pub async fn download_handler(Query(q): Query<DownloadQuery>) -> Response {
    let cfg = settings::umbrel();
    let upstream = http()
        .get(format!("{}/mcp/files/download?path={}", cfg.url, enc(&q.path)))
        .bearer_auth(&cfg.token)
        .send()
        .await;
    let res = match upstream {
        Ok(r) if r.status().is_success() => r,
        Ok(r) => {
            let status = StatusCode::from_u16(r.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
            let text = r.text().await.unwrap_or_default();
            return (status, text).into_response();
        }
        Err(e) => return (StatusCode::BAD_GATEWAY, e.to_string()).into_response(),
    };

    let name = q.path.rsplit('/').next().unwrap_or("download").to_string();
    let inline = q.inline.is_some();
    let upstream_type = res.headers().get(header::CONTENT_TYPE).cloned();
    // Inline previews: pick a safe type from the extension (never text/html, so nothing runs in our origin).
    let guessed = inline.then(|| inline_type(&name)).flatten();
    let is_pdf = guessed == Some("application/pdf");
    let ctype = guessed
        .map(HeaderValue::from_static)
        .or(upstream_type)
        .unwrap_or_else(|| HeaderValue::from_static("application/octet-stream"));
    let len = res.headers().get(header::CONTENT_LENGTH).cloned();
    let disposition = if inline { "inline" } else { "attachment" };
    let ascii: String = name.chars().map(|c| if c.is_ascii_alphanumeric() || ".-_ ".contains(c) { c } else { '_' }).collect();

    let mut out = Response::new(Body::from_stream(res.bytes_stream()));
    let h = out.headers_mut();
    h.insert(header::CONTENT_TYPE, ctype);
    if let Some(len) = len {
        h.insert(header::CONTENT_LENGTH, len);
    }
    if let Ok(v) = HeaderValue::from_str(&format!("{disposition}; filename=\"{ascii}\"; filename*=UTF-8''{}", enc(&name))) {
        h.insert(header::CONTENT_DISPOSITION, v);
    }
    h.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    // Browsers refuse to render PDFs inside a sandboxed document; PDFs run in the viewer's own sandbox.
    if !is_pdf {
        h.insert(header::CONTENT_SECURITY_POLICY, HeaderValue::from_static("sandbox"));
    }
    if inline {
        h.insert(header::CACHE_CONTROL, HeaderValue::from_static("private, max-age=300"));
    }
    out
}

/// Content type for inline previews. Text-like files are always served as plain text.
fn inline_type(name: &str) -> Option<&'static str> {
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase())?;
    Some(match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "bmp" => "image/bmp",
        "ico" => "image/x-icon",
        "heic" => "image/heic",
        "svg" => "image/svg+xml",
        "mp4" | "m4v" => "video/mp4",
        "webm" => "video/webm",
        "mov" => "video/quicktime",
        "mkv" => "video/x-matroska",
        "ogv" => "video/ogg",
        "mp3" => "audio/mpeg",
        "m4a" | "aac" => "audio/mp4",
        "wav" => "audio/wav",
        "flac" => "audio/flac",
        "ogg" | "oga" | "opus" => "audio/ogg",
        "pdf" => "application/pdf",
        "txt" | "md" | "log" | "csv" | "json" | "yml" | "yaml" | "toml" | "ini" | "conf" | "cfg" | "env" | "xml" | "html"
        | "htm" | "css" | "js" | "ts" | "rs" | "py" | "sh" | "go" | "c" | "h" | "cpp" | "java" | "sql" => {
            "text/plain; charset=utf-8"
        }
        _ => return None,
    })
}

#[derive(Deserialize)]
pub struct UploadQuery {
    dir: String,
}

pub async fn upload_handler(Query(q): Query<UploadQuery>, mut multipart: Multipart) -> Response {
    let cfg = settings::umbrel();
    let dir = q.dir.trim_end_matches('/').to_string();
    let mut ok = 0usize;
    let mut errors: Vec<String> = Vec::new();

    loop {
        let mut field = match multipart.next_field().await {
            Ok(Some(f)) => f,
            Ok(None) => break,
            Err(e) => {
                errors.push(e.to_string());
                break;
            }
        };
        let Some(name) = field.file_name().map(str::to_string).filter(|n| !n.is_empty()) else { continue };
        let name = name.rsplit(['/', '\\']).next().unwrap_or(&name).to_string();
        let target = format!("{dir}/{name}");

        // Pump the multipart field into a channel that feeds the upstream request body.
        let (tx, rx) = tokio::sync::mpsc::channel::<Result<Bytes, std::io::Error>>(8);
        let body_stream = futures::stream::unfold(rx, |mut rx| async move { rx.recv().await.map(|item| (item, rx)) });
        let request = http()
            .post(format!("{}/mcp/files/upload?path={}", cfg.url, enc(&target)))
            .bearer_auth(&cfg.token)
            .body(reqwest::Body::wrap_stream(body_stream))
            .send();
        let pump = async move {
            loop {
                match field.chunk().await {
                    Ok(Some(chunk)) => {
                        if tx.send(Ok(chunk)).await.is_err() {
                            break;
                        }
                    }
                    Ok(None) => break,
                    Err(e) => {
                        let _ = tx.send(Err(std::io::Error::other(e.to_string()))).await;
                        break;
                    }
                }
            }
        };
        let (res, ()) = tokio::join!(request, pump);
        match res {
            Ok(r) if r.status().is_success() => ok += 1,
            Ok(r) => {
                let status = r.status();
                let text = r.text().await.unwrap_or_default();
                errors.push(format!("{name}: HTTP {status} {}", text.chars().take(200).collect::<String>()));
            }
            Err(e) => errors.push(format!("{name}: {e}")),
        }
    }

    let msg = if errors.is_empty() {
        format!("{ok} Datei(en) hochgeladen")
    } else {
        format!("{ok} hochgeladen, Fehler: {}", errors.join("; "))
    };
    Redirect::to(&format!("/files?path={}&msg={}", enc(if dir.is_empty() { "/" } else { &dir }), enc(&msg))).into_response()
}


#[derive(Deserialize)]
pub struct RawUploadQuery {
    path: String,
}

/// Plain-text error response the UI shows as-is.
fn fail(status: StatusCode, msg: impl Into<String>) -> Response {
    (status, [(header::CONTENT_TYPE, "text/plain; charset=utf-8")], msg.into()).into_response()
}

/// Checks a virtual umbrelOS target path (`/Home/a/b.txt`).
fn valid_target(path: &str) -> bool {
    path.starts_with('/')
        && !path.ends_with('/')
        && !path.contains('\\')
        && path.split('/').skip(1).all(|seg| !seg.is_empty() && seg != "." && seg != "..")
        && path.split('/').count() >= 3
}

/// `POST /files/upload-raw?path=<full target path>` – the request body is the raw file content.
/// It is streamed straight to umbrelOS, so large files never sit in memory.
pub async fn upload_raw_handler(Query(q): Query<RawUploadQuery>, body: Body) -> Response {
    let cfg = settings::umbrel();
    if cfg.token.is_empty() {
        return fail(StatusCode::SERVICE_UNAVAILABLE, settings::NO_TOKEN);
    }
    let target = q.path.trim().to_string();
    if !valid_target(&target) {
        return fail(StatusCode::BAD_REQUEST, format!("Ungültiger Zielpfad: {target}"));
    }
    let name = target.rsplit('/').next().unwrap_or_default().to_string();
    let res = http()
        .post(format!("{}/mcp/files/upload?path={}", cfg.url, enc(&target)))
        .bearer_auth(&cfg.token)
        .body(reqwest::Body::wrap_stream(body.into_data_stream()))
        .send()
        .await;
    match res {
        Ok(r) if r.status().is_success() => {
            let text = r.text().await.unwrap_or_default();
            (StatusCode::OK, [(header::CONTENT_TYPE, "application/json")], text).into_response()
        }
        Ok(r) => {
            let status = r.status().as_u16();
            let text = r.text().await.unwrap_or_default();
            let lower = text.to_lowercase();
            if status == 409 || lower.contains("exist") || lower.contains("conflict") {
                return fail(
                    StatusCode::CONFLICT,
                    format!("„{name}“ existiert hier bereits – umbrelOS überschreibt keine Dateien. Bitte die vorhandene Datei umbenennen oder in den Papierkorb legen."),
                );
            }
            let detail = upstream_message(&text);
            let code = StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY);
            fail(code, format!("Hochladen von „{name}“ fehlgeschlagen (HTTP {status}){detail}"))
        }
        Err(e) => fail(StatusCode::BAD_GATEWAY, format!("Umbrel nicht erreichbar: {e}")),
    }
}

/// Extracts a readable message from an upstream error body (JSON `{error|message}` or text).
fn upstream_message(text: &str) -> String {
    let msg = serde_json::from_str::<serde_json::Value>(text)
        .ok()
        .and_then(|v| {
            v.get("error")
                .or_else(|| v.get("message"))
                .and_then(|m| m.as_str().map(str::to_string).or_else(|| m.get("message").and_then(|x| x.as_str()).map(str::to_string)))
        })
        .unwrap_or_else(|| text.trim().chars().take(200).collect());
    if msg.is_empty() { String::new() } else { format!(": {msg}") }
}

/// Extra HTTP routes of the file manager (merged into the main router, behind the auth middleware).
pub fn routes() -> axum::Router {
    use axum::routing::post;
    axum::Router::new()
        .route("/files/upload-raw", post(upload_raw_handler).layer(axum::extract::DefaultBodyLimit::disable()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn targets() {
        assert!(valid_target("/Home/a.txt"));
        assert!(valid_target("/Home/Ordner/b c.bin"));
        assert!(!valid_target("/a.txt"));
        assert!(!valid_target("Home/a.txt"));
        assert!(!valid_target("/Home/../etc/passwd"));
        assert!(!valid_target("/Home//a"));
        assert!(!valid_target("/Home/a/"));
    }

    #[test]
    fn inline_types() {
        assert_eq!(inline_type("x.PNG"), Some("image/png"));
        assert_eq!(inline_type("index.html"), Some("text/plain; charset=utf-8"));
        assert_eq!(inline_type("noext"), None);
    }
}

// ---------- Text editor ----------

const EDITOR_MAX: usize = 5 * 1024 * 1024;

pub async fn read_text_file(path: &str) -> Result<crate::api::files::TextFile, String> {
    let cfg = settings::umbrel();
    let mut res = http()
        .get(format!("{}/mcp/files/download?path={}", cfg.url, enc(path)))
        .bearer_auth(&cfg.token)
        .send()
        .await
        .map_err(|e| format!("Umbrel nicht erreichbar: {e}"))?;
    if !res.status().is_success() {
        return Err(format!("Datei konnte nicht geladen werden (HTTP {})", res.status()));
    }
    let mut buf: Vec<u8> = Vec::new();
    while let Some(chunk) = res.chunk().await.map_err(|e| e.to_string())? {
        buf.extend_from_slice(&chunk);
        if buf.len() > EDITOR_MAX {
            return Err("Die Datei ist zu gross für den Editor (max. 5 MB).".into());
        }
    }
    if buf.iter().take(8000).any(|b| *b == 0) {
        return Err("Das ist keine Textdatei (Binärdaten) – sie kann nicht im Editor geöffnet werden.".into());
    }
    let size = buf.len() as u64;
    let content = String::from_utf8(buf).unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned());
    Ok(crate::api::files::TextFile { content, size })
}

/// Uploads bytes; `Ok(Some(path))` on success, `Ok(None)` when the destination already exists.
async fn upload_bytes(path: &str, data: Vec<u8>, keep_both: bool) -> Result<Option<String>, String> {
    let cfg = settings::umbrel();
    let mut url = format!("{}/mcp/files/upload?path={}", cfg.url, enc(path));
    if keep_both {
        url.push_str("&collision=keep-both");
    }
    let res = http().post(url).bearer_auth(&cfg.token).body(data).send().await.map_err(|e| format!("Umbrel nicht erreichbar: {e}"))?;
    let status = res.status();
    let v: serde_json::Value = res.json().await.unwrap_or_default();
    if status.is_success() {
        return Ok(Some(v.get("path").and_then(serde_json::Value::as_str).unwrap_or(path).to_string()));
    }
    let msg = v.get("error").and_then(serde_json::Value::as_str).unwrap_or_default().to_string();
    if msg.contains("already-exists") {
        return Ok(None);
    }
    Err(if msg.is_empty() { format!("Speichern fehlgeschlagen (HTTP {status})") } else { format!("Speichern fehlgeschlagen: {msg}") })
}

fn is_internal(path: &str) -> bool {
    path.starts_with("/Home/") || path.starts_with("/Apps/")
}

pub async fn save_text_file(path: &str, content: String) -> Result<crate::api::files::SavedText, String> {
    use crate::api::files::SavedText;
    let (dir, name) = path.rsplit_once('/').ok_or("Ungültiger Pfad")?;
    if name.is_empty() {
        return Err("Ungültiger Dateiname".into());
    }
    let data = content.into_bytes();
    // New file: plain upload.
    if let Some(p) = upload_bytes(path, data.clone(), false).await? {
        return Ok(SavedText { path: p, copied: false });
    }
    if !is_internal(path) {
        let p = upload_bytes(path, data, true).await?.ok_or("Speichern fehlgeschlagen")?;
        return Ok(SavedText { path: p, copied: true });
    }
    // Internal storage: temp file → old version to Trash → rename.
    let tmp_name = format!(".{name}.nrzedit-{}", super::auth::random_token(4));
    let tmp = format!("{dir}/{tmp_name}");
    upload_bytes(&tmp, data, false).await?.ok_or("Temporäre Datei konnte nicht angelegt werden")?;
    if let Err(e) = super::mcp::call("trash", serde_json::json!({ "path": path })).await {
        let _ = super::mcp::call("trash", serde_json::json!({ "path": tmp })).await;
        return Err(format!("Alte Version konnte nicht in den Papierkorb: {e}"));
    }
    super::mcp::call("rename", serde_json::json!({ "path": tmp, "name": name }))
        .await
        .map_err(|e| format!("Gespeichert als {tmp_name}, Umbenennen fehlgeschlagen: {e}"))?;
    Ok(SavedText { path: path.to_string(), copied: false })
}
