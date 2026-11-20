//! File manager server functions (umbrelOS files via MCP).

use dioxus::prelude::*;

#[cfg(feature = "server")]
use serde_json::{Value, json};

use crate::model::{DirListing, FileEntry};

#[cfg(feature = "server")]
use super::{err, value_to_text};
#[cfg(feature = "server")]
use crate::server::mcp;

/// One page of a directory in umbrelOS order (sorted by name). Keep that order for paging:
/// the next page starts after the last name of this one (`last_file`). The UI sorts itself.
#[server]
pub async fn list_dir(
    path: String,
    last_file: Option<String>,
) -> Result<DirListing, ServerFnError> {
    let mut args = json!({ "path": if path.is_empty() { "/".into() } else { path }, "limit": 200 });
    if let Some(l) = last_file {
        args["lastFile"] = Value::String(l);
    }
    mcp::call_as("list_directory", args).await.map_err(err)
}

/// Hostname of the umbrelOS device (label of the "Umbrel" entry in the sidebar).
#[server]
pub async fn device_name() -> Result<String, ServerFnError> {
    let info: crate::model::SystemInfo = mcp::call_as("get_system_info", json!({}))
        .await
        .map_err(err)?;
    Ok(info.hostname)
}

#[server]
pub async fn search_files(query: String) -> Result<Vec<FileEntry>, ServerFnError> {
    let v = mcp::call("search_files", json!({ "query": query, "limit": 100 }))
        .await
        .map_err(err)?;
    let arr = v
        .get("files")
        .or_else(|| v.get("results"))
        .cloned()
        .unwrap_or(if v.is_array() {
            v.clone()
        } else {
            Value::Array(vec![])
        });
    serde_json::from_value(arr).map_err(err)
}

#[cfg(feature = "server")]
fn tool_args(op: &str, path: &str, arg: &str) -> Result<(&'static str, Value), ServerFnError> {
    Ok(match op {
        "mkdir" => (
            "create_directory",
            json!({ "path": format!("{}/{}", path.trim_end_matches('/'), arg.trim()) }),
        ),
        "rename" => ("rename", json!({ "path": path, "name": arg.trim() })),
        "trash" => ("trash", json!({ "path": path })),
        "move" => (
            "move",
            json!({ "path": path, "toDirectory": arg.trim(), "collision": "error" }),
        ),
        "copy" => (
            "copy",
            json!({ "path": path, "toDirectory": arg.trim(), "collision": "keep-both" }),
        ),
        _ => return Err(err("Unbekannte Dateioperation")),
    })
}

/// Makes MCP errors readable (name conflicts, missing permissions).
#[cfg(feature = "server")]
fn friendly(op: &str, path: &str, e: String) -> String {
    let name = path.rsplit('/').next().unwrap_or(path);
    let lower = e.to_lowercase();
    if lower.contains("exist") || lower.contains("conflict") || lower.contains("collision") {
        return match op {
            "mkdir" => "Ein Ordner mit diesem Namen existiert bereits".into(),
            "rename" => format!("„{name}“: Der neue Name ist schon vergeben"),
            _ => format!("„{name}“: Im Ziel gibt es bereits ein Element mit diesem Namen"),
        };
    }
    format!("„{name}“: {e}")
}

#[server]
pub async fn fs_op(op: String, path: String, arg: String) -> Result<String, ServerFnError> {
    let (tool, args) = tool_args(&op, &path, &arg)?;
    let v = mcp::call(tool, args)
        .await
        .map_err(|e| err(friendly(&op, &path, e)))?;
    Ok(value_to_text(&v))
}

/// Runs one operation (`move`, `copy`, `trash`) for many paths. Returns the number of successes
/// and one readable message per failure.
#[server]
pub async fn fs_batch(
    op: String,
    paths: Vec<String>,
    arg: String,
) -> Result<(usize, Vec<String>), ServerFnError> {
    if !matches!(op.as_str(), "move" | "copy" | "trash") {
        return Err(err("Unbekannte Dateioperation"));
    }
    let mut ok = 0;
    let mut errors = Vec::new();
    for path in paths {
        let (tool, args) = tool_args(&op, &path, &arg)?;
        match mcp::call(tool, args).await {
            Ok(_) => ok += 1,
            Err(e) => errors.push(friendly(&op, &path, e)),
        }
    }
    Ok((ok, errors))
}

/// Creates nested folders below `base` (for folder uploads), parents first. Existing folders are fine.
#[server]
pub async fn make_dirs(base: String, dirs: Vec<String>) -> Result<(), ServerFnError> {
    let base = base.trim_end_matches('/').to_string();
    let mut dirs: Vec<String> = dirs
        .into_iter()
        .map(|d| d.trim_matches('/').to_string())
        .filter(|d| !d.is_empty() && d.split('/').all(|s| !s.is_empty() && s != "." && s != ".."))
        .collect();
    dirs.sort_by_key(|d| (d.matches('/').count(), d.clone()));
    dirs.dedup();
    for d in dirs {
        if let Err(e) =
            mcp::call("create_directory", json!({ "path": format!("{base}/{d}") })).await
        {
            let lower = e.to_lowercase();
            if !(lower.contains("exist") || lower.contains("conflict")) {
                return Err(err(format!(
                    "Ordner „{d}“ konnte nicht angelegt werden: {e}"
                )));
            }
        }
    }
    Ok(())
}

/// Loads the beginning of a text file for the preview (at most `MAX` bytes).
#[server]
pub async fn read_text(path: String) -> Result<(String, bool), ServerFnError> {
    const MAX: usize = 512 * 1024;
    let cfg = crate::server::settings::umbrel();
    let mut res = crate::server::http()
        .get(format!(
            "{}/mcp/files/download?path={}",
            cfg.url,
            urlencoding::encode(&path)
        ))
        .bearer_auth(&cfg.token)
        .send()
        .await
        .map_err(|e| err(format!("Umbrel nicht erreichbar: {e}")))?;
    if !res.status().is_success() {
        return Err(err(format!(
            "Datei konnte nicht geladen werden (HTTP {})",
            res.status()
        )));
    }
    let mut buf: Vec<u8> = Vec::new();
    let mut truncated = false;
    while let Some(chunk) = res.chunk().await.map_err(err)? {
        buf.extend_from_slice(&chunk);
        if buf.len() >= MAX {
            buf.truncate(MAX);
            truncated = true;
            break;
        }
    }
    Ok((String::from_utf8_lossy(&buf).into_owned(), truncated))
}

/// A text file opened in the editor.
#[derive(serde::Serialize, serde::Deserialize, Clone, PartialEq, Debug, Default)]
pub struct TextFile {
    pub content: String,
    pub size: u64,
}

/// Result of saving in the editor.
#[derive(serde::Serialize, serde::Deserialize, Clone, PartialEq, Debug, Default)]
pub struct SavedText {
    /// Where the text now lives (differs from the request when saved as a copy).
    pub path: String,
    /// `true` when umbrelOS could not replace the file (USB/network storage) and a copy was written.
    pub copied: bool,
}

/// Loads a whole text file for the editor (max. 5 MB, binary files are refused).
#[server]
pub async fn read_text_file(path: String) -> Result<TextFile, ServerFnError> {
    crate::server::files::read_text_file(&path)
        .await
        .map_err(err)
}

/// Saves editor content. umbrelOS never overwrites files, so for internal storage the new text is
/// uploaded next to the file, the old version goes to the Trash (kept as backup) and the new file
/// takes its name. On USB/network storage (no Trash) a copy is written instead.
#[server]
pub async fn save_text_file(path: String, content: String) -> Result<SavedText, ServerFnError> {
    crate::server::files::save_text_file(&path, content)
        .await
        .map_err(err)
}
