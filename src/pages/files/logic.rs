//! Pure helpers of the file manager (paths, file kinds, sorting, selection). No browser APIs.

use std::cmp::Ordering;

use crate::model::FileEntry;

/// Parent directory of a virtual path (`/Home/a` → `/Home`, `/Home` → `/`).
pub fn parent(path: &str) -> String {
    match path.trim_end_matches('/').rsplit_once('/') {
        Some(("", _)) | None => "/".into(),
        Some((p, _)) => p.into(),
    }
}

pub fn join(dir: &str, name: &str) -> String {
    format!("{}/{}", dir.trim_end_matches('/'), name.trim_matches('/'))
}

pub fn name_of(path: &str) -> String {
    path.trim_end_matches('/').rsplit('/').next().unwrap_or(path).to_string()
}

/// `true` if `path` is `ancestor` itself or lies below it.
pub fn is_within(path: &str, ancestor: &str) -> bool {
    let a = ancestor.trim_end_matches('/');
    path == a || path.strip_prefix(a).is_some_and(|rest| rest.starts_with('/'))
}

/// Rewrites `list` after `moved` went into `dir`; entries inside trashed items (`dir == None`) are dropped.
pub fn remap_paths(list: &[String], moved: &[String], dir: Option<&str>) -> Vec<String> {
    list.iter()
        .filter_map(|f| match moved.iter().find(|p| is_within(f, p)) {
            None => Some(f.clone()),
            Some(p) => dir.map(|d| format!("{}{}", join(d, &name_of(p)), &f[p.trim_end_matches('/').len()..])),
        })
        .collect()
}

/// Breadcrumb segments: `(name, full path)`.
pub fn crumbs(path: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut acc = String::new();
    for seg in path.split('/').filter(|s| !s.is_empty()) {
        acc.push('/');
        acc.push_str(seg);
        out.push((seg.to_string(), acc.clone()));
    }
    out
}

/// Length of the name without extension in UTF-16 units (for selecting it in the rename field).
pub fn stem_utf16_len(name: &str, is_dir: bool) -> u32 {
    let stem = match name.rsplit_once('.') {
        Some((s, _)) if !is_dir && !s.is_empty() => s,
        _ => name,
    };
    stem.encode_utf16().count() as u32
}

/// Checks a new file/folder name.
pub fn validate_name(name: &str) -> Result<String, &'static str> {
    let n = name.trim();
    if n.is_empty() {
        return Err("Bitte einen Namen eingeben");
    }
    if n == "." || n == ".." || n.contains('/') || n.contains('\\') {
        return Err("Der Name darf keine / oder \\ enthalten");
    }
    Ok(n.to_string())
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Folder,
    Image,
    Video,
    Audio,
    Pdf,
    Text,
    Code,
    Archive,
    Other,
}

pub fn kind_of(e: &FileEntry) -> Kind {
    if e.is_dir() { Kind::Folder } else { kind_by_name(&e.name) }
}

pub fn kind_by_name(name: &str) -> Kind {
    let lower = name.to_ascii_lowercase();
    // Well-known text files without (useful) extension.
    if matches!(
        lower.as_str(),
        "dockerfile" | "makefile" | "readme" | "license" | "changelog" | "procfile" | "justfile" | "caddyfile"
            | ".gitignore" | ".dockerignore" | ".env" | ".editorconfig" | ".npmrc" | ".bashrc" | ".zshrc" | ".profile"
    ) || lower.starts_with(".env.")
    {
        return Kind::Text;
    }
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "avif" | "bmp" | "svg" | "ico" | "heic" => Kind::Image,
        "mp4" | "m4v" | "webm" | "mov" | "mkv" | "ogv" | "avi" => Kind::Video,
        "mp3" | "m4a" | "aac" | "wav" | "flac" | "ogg" | "oga" | "opus" => Kind::Audio,
        "pdf" => Kind::Pdf,
        "txt" | "md" | "markdown" | "log" | "csv" | "tsv" | "ini" | "conf" | "cfg" | "env" | "nfo" | "srt" | "vtt"
        | "properties" | "rst" | "tex" | "lock" | "list" | "service" | "desktop" => Kind::Text,
        "json" | "jsonc" | "json5" | "yml" | "yaml" | "toml" | "xml" | "plist" | "html" | "htm" | "xhtml" | "css" | "scss"
        | "sass" | "less" | "js" | "mjs" | "cjs" | "jsx" | "ts" | "tsx" | "vue" | "svelte" | "rs" | "py" | "rb" | "php"
        | "sh" | "bash" | "zsh" | "fish" | "ps1" | "bat" | "cmd" | "go" | "c" | "h" | "cpp" | "hpp" | "cc" | "cs"
        | "java" | "kt" | "kts" | "swift" | "dart" | "lua" | "pl" | "r" | "sql" | "graphql" | "proto" | "gradle"
        | "dockerfile" | "nix" | "tf" | "hcl" => Kind::Code,
        "zip" | "tar" | "gz" | "tgz" | "bz2" | "xz" | "7z" | "rar" | "zst" => Kind::Archive,
        _ => Kind::Other,
    }
}

/// Lower-case file extension (`""` for folders and names without one).
pub fn ext_of(name: &str) -> String {
    match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() && ext.len() <= 6 => ext.to_ascii_lowercase(),
        _ => String::new(),
    }
}

/// "Typ" column: `Ordner`, `PDF-Datei`, `Datei`.
pub fn type_label(e: &FileEntry) -> String {
    if e.is_dir() {
        return "Ordner".into();
    }
    let ext = ext_of(&e.name);
    if ext.is_empty() { "Datei".into() } else { format!("{}-Datei", ext.to_ascii_uppercase()) }
}

/// Real folder that can receive uploads / new folders (not `/` and not a virtual view like "Kürzlich").
pub fn is_target_dir(path: &str) -> bool {
    path.starts_with('/') && path != "/"
}

/// Whether the browser can show an `<img>` thumbnail.
pub fn has_thumbnail(name: &str) -> bool {
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "gif" | "webp" | "avif" | "bmp" | "svg" | "ico")
}

/// Kinds the preview dialog can display.
/// Opens in the preview dialog (media, PDF). Text and code open in the editor instead.
pub fn previewable(k: Kind) -> bool {
    matches!(k, Kind::Image | Kind::Video | Kind::Audio | Kind::Pdf)
}

/// Opens in the editor by default.
pub fn editable(k: Kind) -> bool {
    matches!(k, Kind::Text | Kind::Code)
}

/// "Im Editor öffnen" is offered for everything that is not obviously binary.
pub fn may_edit(k: Kind) -> bool {
    matches!(k, Kind::Text | Kind::Code | Kind::Other)
}

/// Language label for the editor status bar.
pub fn language_of(name: &str) -> &'static str {
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "md" | "markdown" => "Markdown",
        "json" | "jsonc" | "json5" => "JSON",
        "yml" | "yaml" => "YAML",
        "toml" => "TOML",
        "html" | "htm" | "xhtml" => "HTML",
        "xml" | "plist" => "XML",
        "css" | "scss" | "sass" | "less" => "CSS",
        "js" | "mjs" | "cjs" | "jsx" => "JavaScript",
        "ts" | "tsx" => "TypeScript",
        "rs" => "Rust",
        "py" => "Python",
        "sh" | "bash" | "zsh" | "fish" => "Shell",
        "go" => "Go",
        "sql" => "SQL",
        "csv" | "tsv" => "CSV",
        "log" => "Log",
        _ => "Text",
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ViewMode {
    Grid,
    List,
}

impl ViewMode {
    pub fn key(self) -> &'static str {
        match self {
            Self::Grid => "grid",
            Self::List => "list",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "grid" => Some(Self::Grid),
            "list" => Some(Self::List),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SortKey {
    Name,
    Size,
    Modified,
    Type,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Sort {
    pub key: SortKey,
    pub asc: bool,
}

impl Default for Sort {
    fn default() -> Self {
        Self { key: SortKey::Name, asc: true }
    }
}

impl Sort {
    pub fn encode(self) -> String {
        let k = match self.key {
            SortKey::Name => "name",
            SortKey::Size => "size",
            SortKey::Modified => "modified",
            SortKey::Type => "type",
        };
        format!("{k}:{}", if self.asc { "asc" } else { "desc" })
    }
    pub fn parse(s: &str) -> Option<Self> {
        let (k, d) = s.split_once(':')?;
        let key = match k {
            "name" => SortKey::Name,
            "size" => SortKey::Size,
            "modified" => SortKey::Modified,
            "type" => SortKey::Type,
            _ => return None,
        };
        Some(Self { key, asc: d != "desc" })
    }
    /// Clicking a column: same column flips the direction, a new one starts with a sensible default.
    pub fn toggle(self, key: SortKey) -> Self {
        if self.key == key { Self { key, asc: !self.asc } } else { Self { key, asc: matches!(key, SortKey::Name | SortKey::Type) } }
    }
}

/// Case-insensitive comparison that orders embedded numbers numerically (`2` < `10`).
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (mut ai, mut bi) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (ai.peek().copied(), bi.peek().copied()) {
            (None, None) => return a.cmp(b),
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let mut na = String::new();
                while let Some(c) = ai.peek().copied().filter(char::is_ascii_digit) {
                    na.push(c);
                    ai.next();
                }
                let mut nb = String::new();
                while let Some(c) = bi.peek().copied().filter(char::is_ascii_digit) {
                    nb.push(c);
                    bi.next();
                }
                let (ta, tb) = (na.trim_start_matches('0'), nb.trim_start_matches('0'));
                let ord = ta.len().cmp(&tb.len()).then_with(|| ta.cmp(tb));
                if ord != Ordering::Equal {
                    return ord;
                }
            }
            (Some(x), Some(y)) => {
                let ord = x.to_lowercase().cmp(y.to_lowercase());
                if ord != Ordering::Equal {
                    return ord;
                }
                ai.next();
                bi.next();
            }
        }
    }
}

/// Folders first, then by the chosen column.
pub fn sort_entries(list: &mut [FileEntry], sort: Sort) {
    list.sort_by(|a, b| {
        let by = match sort.key {
            SortKey::Name => natural_cmp(&a.name, &b.name),
            SortKey::Size => a.size.unwrap_or(0.0).total_cmp(&b.size.unwrap_or(0.0)),
            SortKey::Modified => a.modified.unwrap_or(0.0).total_cmp(&b.modified.unwrap_or(0.0)),
            SortKey::Type => ext_of(&a.name).cmp(&ext_of(&b.name)),
        }
        .then_with(|| natural_cmp(&a.name, &b.name));
        b.is_dir().cmp(&a.is_dir()).then(if sort.asc { by } else { by.reverse() })
    });
}

/// Paths between `anchor` and `target` (inclusive) in display order; just `target` without anchor.
pub fn range(order: &[String], anchor: Option<&str>, target: &str) -> Vec<String> {
    let t = order.iter().position(|p| p == target);
    let a = anchor.and_then(|a| order.iter().position(|p| p == a));
    match (a, t) {
        (Some(a), Some(t)) => order[a.min(t)..=a.max(t)].to_vec(),
        _ => vec![target.to_string()],
    }
}

#[cfg_attr(not(feature = "web"), allow(dead_code))]
/// Folders that must exist before uploading files with these relative paths (`a/b/c.txt` → `a`, `a/b`).
pub fn rel_dirs(rel_paths: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for p in rel_paths {
        let segs: Vec<&str> = p.split('/').filter(|s| !s.is_empty()).collect();
        for i in 1..segs.len() {
            let d = segs[..i].join("/");
            if !out.contains(&d) {
                out.push(d);
            }
        }
    }
    out
}

/// Whether moving `paths` into `dir` makes sense (not into itself/below itself, not where they already are).
pub fn can_move_into(paths: &[String], dir: &str) -> bool {
    !paths.is_empty()
        && is_target_dir(dir)
        && paths.iter().all(|p| !is_within(dir, p))
        && paths.iter().any(|p| parent(p) != dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remap() {
        let favs: Vec<String> = ["/Home/A", "/Home/A/Sub", "/Home/AB", "/Home/C"].iter().map(|s| s.to_string()).collect();
        let moved = vec!["/Home/A".to_string()];
        assert_eq!(remap_paths(&favs, &moved, Some("/Home/C")), ["/Home/C/A", "/Home/C/A/Sub", "/Home/AB", "/Home/C"]);
        assert_eq!(remap_paths(&favs, &moved, None), ["/Home/AB", "/Home/C"]);
    }

    fn fe(name: &str, dir: bool, size: f64) -> FileEntry {
        FileEntry {
            name: name.into(),
            path: format!("/Home/{name}"),
            kind: if dir { "directory".into() } else { "file".into() },
            size: Some(size),
            modified: Some(size),
        }
    }

    #[test]
    fn paths() {
        assert_eq!(parent("/Home/a/b"), "/Home/a");
        assert_eq!(parent("/Home"), "/");
        assert_eq!(join("/Home/", "x"), "/Home/x");
        assert_eq!(name_of("/Home/a.txt"), "a.txt");
        assert!(is_within("/Home/a/b", "/Home/a"));
        assert!(is_within("/Home/a", "/Home/a"));
        assert!(!is_within("/Home/ab", "/Home/a"));
        assert_eq!(crumbs("/Home/x").len(), 2);
        assert_eq!(crumbs("/Home/x")[1].1, "/Home/x");
    }

    #[test]
    fn names() {
        assert_eq!(stem_utf16_len("bild.jpg", false), 4);
        assert_eq!(stem_utf16_len(".env", false), 4);
        assert_eq!(stem_utf16_len("a.b", true), 3);
        assert!(validate_name(" a/b ").is_err());
        assert_eq!(validate_name(" neu ").unwrap(), "neu");
    }

    #[test]
    fn natural() {
        assert_eq!(natural_cmp("file2", "file10"), Ordering::Less);
        assert_eq!(natural_cmp("B", "a"), Ordering::Greater);
        assert_eq!(natural_cmp("a", "a1"), Ordering::Less);
    }

    #[test]
    fn sorting() {
        let mut v = vec![fe("b", false, 1.0), fe("z", true, 5.0), fe("a", false, 9.0), fe("c", true, 0.0)];
        sort_entries(&mut v, Sort::default());
        let names: Vec<_> = v.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["c", "z", "a", "b"]);
        sort_entries(&mut v, Sort { key: SortKey::Size, asc: false });
        let names: Vec<_> = v.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["z", "c", "a", "b"]);
        assert_eq!(Sort::parse(&Sort { key: SortKey::Modified, asc: false }.encode()), Some(Sort { key: SortKey::Modified, asc: false }));
    }

    #[test]
    fn ranges_and_dirs() {
        let order: Vec<String> = ["a", "b", "c", "d"].iter().map(|s| s.to_string()).collect();
        assert_eq!(range(&order, Some("d"), "b"), ["b", "c", "d"]);
        assert_eq!(range(&order, None, "c"), ["c"]);
        assert_eq!(rel_dirs(&["x/y/z.txt".into(), "x/a.txt".into(), "top.txt".into()]), ["x", "x/y"]);
    }

    #[test]
    fn moving() {
        let p = vec!["/Home/a".to_string()];
        assert!(!can_move_into(&p, "/Home/a/sub"));
        assert!(!can_move_into(&p, "/Home"));
        assert!(can_move_into(&p, "/Home/b"));
        assert!(!can_move_into(&p, "/"));
        assert!(!can_move_into(&p, "recent"));
    }

    #[test]
    fn types() {
        assert_eq!(ext_of("a.PDF"), "pdf");
        assert_eq!(ext_of(".env"), "");
        assert_eq!(type_label(&fe("x.jpg", false, 1.0)), "JPG-Datei");
        assert_eq!(type_label(&fe("x", true, 1.0)), "Ordner");
        let mut v = vec![fe("b.zip", false, 1.0), fe("a.txt", false, 1.0), fe("c.jpg", false, 1.0)];
        sort_entries(&mut v, Sort { key: SortKey::Type, asc: true });
        let names: Vec<_> = v.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["c.jpg", "a.txt", "b.zip"]);
        assert!(is_target_dir("/Home") && !is_target_dir("/") && !is_target_dir("recent"));
    }
}
