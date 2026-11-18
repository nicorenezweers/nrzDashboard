//! Build- und Release-Aufgaben fuer nrzDashboard.
//!
//!   cargo xtask release 1.0.1 "Kurze Beschreibung"
//!
//! `release` baut nrzDashboard fuer umbrelOS (Linux amd64 + arm64), legt die fertigen Dateien auf den
//! Zweig `umbrel` und pusht den Tag `umbrel-<version>`. nrzGit traegt die Version dann per
//! nrzStore.json selbst im nrzStore ein (RELEASE in hooks/pre-start, version, releaseNotes,
//! Store-Version).
//!
//! Voraussetzungen: dx (dioxus-cli 0.7), zig + cargo-zigbuild (Linker fuer Linux), Rust-Targets
//! x86_64-unknown-linux-gnu und aarch64-unknown-linux-gnu, git, tar, gzip, shasum.

use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

type Result<T> = std::result::Result<T, String>;

const WEB: &str = "target/dx/nrz_dashboard/release/web";
const CSS: &str = "assets/tailwind.generated.css";

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let res = match args.first().map(String::as_str) {
        Some("release") => release(&args[1..]),
        _ => {
            eprintln!("Aufruf: cargo xtask release <version> [\"Beschreibung\"]");
            return ExitCode::from(2);
        }
    };
    match res {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

// ---------- Hilfen ----------

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("Workspace-Wurzel").to_path_buf()
}

/// Fuehrt ein Programm aus und bricht bei Fehler ab.
fn run(cmd: &mut Command) -> Result<()> {
    let st = cmd.status().map_err(|e| format!("{:?} laesst sich nicht starten: {e}", cmd.get_program()))?;
    if st.success() { Ok(()) } else { Err(format!("{:?} fehlgeschlagen ({st})", cmd.get_program())) }
}

/// Fuehrt ein Programm aus und liefert stdout (ohne Zeilenende), `None` bei Fehler.
fn output(cmd: &mut Command) -> Option<String> {
    let out = cmd.stderr(Stdio::null()).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim_end().to_string())
}

fn git<I: IntoIterator<Item = S>, S: AsRef<OsStr>>(args: I) -> Command {
    let mut c = Command::new("git");
    c.args(args);
    c
}

fn quiet(mut c: Command) -> Command {
    c.stdout(Stdio::null()).stderr(Stdio::null());
    c
}

fn tempdir(name: &str) -> Result<PathBuf> {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let p = env::temp_dir().join(format!("nrz-{name}-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&p).map_err(|e| format!("{}: {e}", p.display()))?;
    Ok(p)
}

/// Loescht temporaere Verzeichnisse beim Verlassen (auch bei Fehlern).
struct Cleanup(Vec<PathBuf>);
impl Drop for Cleanup {
    fn drop(&mut self) {
        for p in &self.0 {
            let _ = fs::remove_dir_all(p);
        }
    }
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty() && hay.windows(needle.len()).any(|w| w == needle)
}

fn read(p: impl AsRef<Path>) -> Result<Vec<u8>> {
    fs::read(p.as_ref()).map_err(|e| format!("{}: {e}", p.as_ref().display()))
}

/// Alle Dateien unter `dir`, relativ und sortiert.
fn list_files(dir: &Path) -> Result<Vec<String>> {
    fn walk(base: &Path, dir: &Path, out: &mut Vec<String>) -> Result<()> {
        for e in fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
            let p = e.map_err(|e| e.to_string())?.path();
            if p.is_dir() {
                walk(base, &p, out)?;
            } else {
                out.push(p.strip_prefix(base).unwrap_or(&p).to_string_lossy().into_owned());
            }
        }
        Ok(())
    }
    let mut v = Vec::new();
    walk(dir, dir, &mut v)?;
    v.sort();
    Ok(v)
}

/// Sucht rekursiv eine Datei, deren Name mit `prefix` beginnt und auf `suffix` endet.
fn find_file(dir: &Path, prefix: &str, suffix: &str) -> Option<PathBuf> {
    let mut entries: Vec<_> = fs::read_dir(dir).ok()?.flatten().map(|e| e.path()).collect();
    entries.sort();
    for p in &entries {
        let name = p.file_name().and_then(OsStr::to_str).unwrap_or("");
        if p.is_file() && name.starts_with(prefix) && name.ends_with(suffix) {
            return Some(p.clone());
        }
    }
    entries.iter().filter(|p| p.is_dir()).find_map(|p| find_file(p, prefix, suffix))
}

// ---------- Werkzeuge finden ----------

/// dioxus-cli finden (`dx` im PATH kann auch Denos dx sein).
fn find_dx() -> Result<PathBuf> {
    let mut cands: Vec<PathBuf> = Vec::new();
    if let Some(d) = env::var_os("DX").filter(|d| !d.is_empty()) {
        cands.push(d.into());
    }
    if let Some(path) = env::var_os("PATH") {
        cands.extend(env::split_paths(&path).map(|p| p.join("dx")));
    }
    if let Some(home) = env::var_os("HOME") {
        cands.push(Path::new(&home).join(".cargo/bin/dx"));
    }
    cands
        .into_iter()
        .find(|c| output(Command::new(c).arg("--version")).is_some_and(|v| v.to_lowercase().contains("dioxus")))
        .ok_or_else(|| "dioxus-cli (dx) nicht gefunden – cargo binstall dioxus-cli".into())
}

/// Verzeichnis mit den zig-Linker-Wrappern von cargo-zigbuild (glibc 2.31 laeuft auf jedem aktuellen Debian).
fn zig_wrappers() -> Result<PathBuf> {
    if output(Command::new("cargo").args(["zigbuild", "--help"])).is_none() {
        return Err("cargo-zigbuild fehlt".into());
    }
    let home = PathBuf::from(env::var_os("HOME").unwrap_or_default());
    let caches = [home.join("Library/Caches/cargo-zigbuild"), home.join(".cache/cargo-zigbuild")];
    let find = || caches.iter().find_map(|d| find_file(d, "zigcc-x86_64-unknown-linux-gnu.2.31-", ".sh"));
    let wrap = match find() {
        Some(w) => w,
        None => {
            // Einmal anstossen, damit cargo-zigbuild die Wrapper anlegt.
            for t in ["x86_64-unknown-linux-gnu.2.31", "aarch64-unknown-linux-gnu.2.31"] {
                let mut c = quiet(Command::new("cargo"));
                let _ = c.args(["zigbuild", "--release", "--features", "server", "--target", t]).status();
            }
            find().ok_or("zig-Linker-Wrapper von cargo-zigbuild nicht gefunden")?
        }
    };
    Ok(wrap.parent().expect("Wrapper-Verzeichnis").to_path_buf())
}

fn css_hash() -> Option<String> {
    output(Command::new("shasum").arg(CSS))
}

// ---------- Release ----------

struct Ctx {
    dx: PathBuf,
    wdir: PathBuf,
    out: PathBuf,
    lists: PathBuf,
}

/// Baut Server + Oberflaeche fuer eine Architektur und prueft das Ergebnis.
fn build(ctx: &Ctx, triple: &str, arch: &str) -> Result<()> {
    let up = triple.to_uppercase().replace('-', "_");
    let low = triple.replace('-', "_");
    let cc = find_file(&ctx.wdir, &format!("zigcc-{triple}.2.31-"), ".sh")
        .ok_or_else(|| format!("zig-Wrapper fuer {triple} fehlt"))?;
    let ar = ctx.wdir.join("ar");
    println!("==> Baue {arch} ({triple})");
    let _ = fs::remove_dir_all("target/dx/nrz_dashboard/release");
    run(Command::new(&ctx.dx)
        .env(format!("CARGO_TARGET_{up}_LINKER"), &cc)
        .env(format!("CC_{low}"), &cc)
        .env(format!("CC_{triple}"), &cc)
        .env(format!("AR_{low}"), &ar)
        .env(format!("AR_{triple}"), &ar)
        .args(["bundle", "--platform", "web", "--fullstack", "--release", "@server", "--target", triple, "--features", "server"]))?;

    // Ohne das Feature `server` baut dx mit eigenem @server-Target einen Client und der Container
    // stirbt mit "cannot call wasm-bindgen imported functions on non-wasm targets".
    let server = Path::new(WEB).join("server");
    let bin = read(&server)?;
    if !contains(&bin, b"hash-password") {
        return Err(format!("Server-Binary fuer {arch} enthaelt keinen Server-Teil – Abbruch."));
    }
    // Server und Oberflaeche muessen dieselben Asset-Namen kennen.
    let assets = Path::new(WEB).join("public/assets");
    let css = find_file(&assets, "tailwind", ".css").ok_or("Tailwind-CSS fehlt in der Oberflaeche")?;
    let name = css.file_name().and_then(OsStr::to_str).unwrap_or_default();
    let hash = name.trim_end_matches(".css").rsplit_once('-').map_or("", |(_, h)| h);
    if !contains(&bin, hash.as_bytes()) {
        return Err(format!("Server-Binary fuer {arch} kennt {name} nicht – Abbruch."));
    }

    let files = list_files(&Path::new(WEB).join("public"))?;
    fs::write(ctx.lists.join(format!("{arch}.txt")), files.join("\n")).map_err(|e| e.to_string())?;
    let gz = fs::File::create(ctx.out.join(format!("nrzdashboard-{arch}.gz"))).map_err(|e| e.to_string())?;
    run(Command::new("gzip").args(["-9", "-c"]).arg(&server).stdout(gz))
}

fn release(args: &[String]) -> Result<()> {
    let version = args.first().ok_or("Version fehlt, z.B. 1.0.1")?.clone();
    let notes = args.get(1).cloned().unwrap_or_else(|| format!("nrzDashboard {version}"));
    let tag = format!("umbrel-{version}");
    env::set_current_dir(root()).map_err(|e| e.to_string())?;

    if !output(&mut git(["status", "--porcelain"])).unwrap_or_default().is_empty() {
        return Err("Arbeitsverzeichnis nicht sauber – erst committen.".into());
    }
    if quiet(git(["rev-parse", "-q", "--verify", &format!("refs/tags/{tag}")])).status().is_ok_and(|s| s.success()) {
        return Err(format!("Tag {tag} gibt es schon."));
    }

    let ctx = Ctx { dx: find_dx()?, wdir: zig_wrappers()?, out: tempdir("out")?, lists: tempdir("lists")? };
    let mut cleanup = Cleanup(vec![ctx.out.clone(), ctx.lists.clone()]);

    let css_before = css_hash();
    build(&ctx, "x86_64-unknown-linux-gnu", "amd64")?;
    let css_changed = css_hash() != css_before;
    if css_changed {
        // dx hat die Tailwind-Ausgabe neu erzeugt: mit dem jetzt stabilen CSS noch einmal bauen,
        // damit beide Architekturen dieselben Asset-Namen haben.
        println!("==> Tailwind-Ausgabe aktualisiert, baue amd64 erneut");
        build(&ctx, "x86_64-unknown-linux-gnu", "amd64")?;
    }
    run(Command::new("tar")
        .env("COPYFILE_DISABLE", "1")
        .arg("--no-xattrs")
        .arg("-czf")
        .arg(ctx.out.join("public.tar.gz"))
        .arg("-C")
        .arg(Path::new(WEB).join("public"))
        .arg("."))?;
    build(&ctx, "aarch64-unknown-linux-gnu", "arm64")?;
    if read(ctx.lists.join("amd64.txt"))? != read(ctx.lists.join("arm64.txt"))? {
        return Err("Oberflaeche von amd64 und arm64 unterscheidet sich – Abbruch.".into());
    }
    let sums = fs::File::create(ctx.out.join("SHA256SUMS")).map_err(|e| e.to_string())?;
    run(Command::new("shasum")
        .current_dir(&ctx.out)
        .args(["-a", "256", "nrzdashboard-amd64.gz", "nrzdashboard-arm64.gz", "public.tar.gz"])
        .stdout(sums))?;

    // Zweig `umbrel` in einem eigenen Worktree aktualisieren (nur fertige Dateien).
    let wt = tempdir("umbrel")?;
    let wts = wt.to_string_lossy().into_owned();
    let _ = quiet(git(["fetch", "origin", "umbrel"])).status();
    if quiet(git(["rev-parse", "-q", "--verify", "origin/umbrel"])).status().is_ok_and(|s| s.success()) {
        run(&mut git(["worktree", "add", "-q", &wts, "-B", "umbrel", "origin/umbrel"]))?;
    } else {
        run(&mut git(["worktree", "add", "-q", "--detach", &wts]))?;
        run(git(["checkout", "-q", "--orphan", "umbrel"]).current_dir(&wt))?;
        run(git(["rm", "-rqf", "."]).current_dir(&wt))?;
    }
    let publish = || -> Result<()> {
        for e in fs::read_dir(&ctx.out).map_err(|e| e.to_string())?.flatten() {
            fs::copy(e.path(), wt.join(e.file_name())).map_err(|e| e.to_string())?;
        }
        let readme = output(&mut git(["show", "HEAD:scripts/umbrel-README.md"])).ok_or("scripts/umbrel-README.md fehlt")?;
        fs::write(wt.join("README.md"), readme + "\n").map_err(|e| e.to_string())?;
        run(git(["add", "-A"]).current_dir(&wt))?;
        run(git(["commit", "-qm", &format!("nrzDashboard {version}: Umbrel-Paket")]).current_dir(&wt))?;
        run(git(["tag", "-a", &tag, "-m", &notes]).current_dir(&wt))?;
        run(git(["push", "-q", "origin", "umbrel"]).current_dir(&wt))?;
        run(git(["push", "-q", "origin", &tag]).current_dir(&wt))
    };
    let res = publish();
    let _ = git(["worktree", "remove", "--force", &wts]).status();
    cleanup.0.push(wt);
    res?;

    println!("==> {tag} veroeffentlicht. nrzGit aktualisiert den nrzStore.");
    if css_changed {
        eprintln!("Hinweis: {CSS} wurde neu erzeugt – bitte committen.");
    }
    Ok(())
}
