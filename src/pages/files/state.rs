//! Shared state of the file manager (signals in one `Copy` handle provided via context) and its actions.

use std::collections::BTreeSet;

use dioxus::core::Runtime;
use dioxus::prelude::*;

use super::logic::{self, Sort, ViewMode};
use super::upload::UploadItem;
use super::web;
use crate::api::files::{device_name, fs_batch, fs_op, list_dir, search_files};
use crate::app::Route;
use crate::model::FileEntry;
use crate::utils::{enc, err_msg};

pub const KEY_VIEW: &str = "nrz.files.view";
pub const KEY_SORT: &str = "nrz.files.sort";
pub const KEY_FAVS: &str = "nrz.files.favs";
pub const KEY_RECENT: &str = "nrz.files.recent";
/// Virtual path of the "Kürzlich" view (recently opened files, kept in localStorage).
pub const RECENT: &str = "recent";
const RECENT_MAX: usize = 40;
/// Favourites until the user changes them (only shown if they exist in /Home).
const DEFAULT_FAVS: [&str; 4] = ["/Home/Downloads", "/Home/Documents", "/Home/Photos", "/Home/Videos"];

#[derive(Clone, PartialEq, Debug)]
pub struct MenuState {
    pub x: f64,
    pub y: f64,
    pub path: String,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PickOp {
    Move,
    Copy,
}

#[derive(Clone, PartialEq, Debug)]
pub struct PickerState {
    pub op: PickOp,
    pub paths: Vec<String>,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Toast {
    pub id: u64,
    pub text: String,
    pub error: bool,
}

pub fn download_url(path: &str) -> String {
    format!("/files/download?path={}", enc(path))
}

pub fn inline_url(e: &FileEntry) -> String {
    format!("/files/download?path={}&inline=1&v={}", enc(&e.path), e.modified.unwrap_or(0.0) as u64)
}

#[derive(Clone, Copy)]
pub struct Fm {
    scope: ScopeId,
    pub path: Signal<String>,
    /// Loaded entries in umbrelOS order (needed for paging); sorted only for display.
    pub entries: Signal<Vec<FileEntry>>,
    pub loaded: Signal<bool>,
    pub has_more: Signal<bool>,
    pub loading_more: Signal<bool>,
    pub pages: Signal<usize>,
    pub error: Signal<Option<String>>,
    generation: Signal<u64>,
    pub selected: Signal<BTreeSet<String>>,
    pub anchor: Signal<Option<String>>,
    pub view: Signal<ViewMode>,
    pub sort: Signal<Sort>,
    pub renaming: Signal<Option<String>>,
    pub menu: Signal<Option<MenuState>>,
    pub preview: Signal<Option<String>>,
    /// File open in the text editor.
    pub editor: Signal<Option<String>>,
    pub picker: Signal<Option<PickerState>>,
    pub new_folder: Signal<bool>,
    /// Paths dragged inside the app.
    pub drag: Signal<Option<Vec<String>>>,
    pub drop_target: Signal<Option<String>>,
    /// dragenter/dragleave balance of an external (OS) file drag over the page.
    pub ext_drag: Signal<i32>,
    pub select_mode: Signal<bool>,
    pub touch: Signal<bool>,
    pub query: Signal<String>,
    pub results: Signal<Option<Result<Vec<FileEntry>, String>>>,
    pub searching: Signal<bool>,
    search_gen: Signal<u64>,
    pub toasts: Signal<Vec<Toast>>,
    pub uploads: Signal<Vec<UploadItem>>,
    pub roots: Signal<Vec<FileEntry>>,
    /// Entry to select after the next folder change ("Im Ordner zeigen").
    pub reveal: Signal<Option<String>>,
    /// Own back/forward history of visited folders.
    pub history: Signal<Vec<String>>,
    pub hist_idx: Signal<usize>,
    /// Favourite folders; `None` = defaults.
    pub favs: Signal<Option<Vec<String>>>,
    /// Subfolders of /Home, /External, /Network for the sidebar.
    pub home_dirs: Signal<Vec<FileEntry>>,
    pub ext_dirs: Signal<Vec<FileEntry>>,
    pub net_dirs: Signal<Vec<FileEntry>>,
    /// Hostname of the device ("Umbrel" entry).
    pub device: Signal<String>,
    /// Sidebar drawer on phones.
    pub sidebar_open: Signal<bool>,
}

impl Fm {
    /// Defaults only – browser settings are applied by [`Fm::init_client`] after hydration.
    pub fn new(path: String) -> Self {
        Self {
            scope: dioxus::core::current_scope_id(),
            path: Signal::new(path),
            entries: Signal::new(Vec::new()),
            loaded: Signal::new(false),
            has_more: Signal::new(false),
            loading_more: Signal::new(false),
            pages: Signal::new(1),
            error: Signal::new(None),
            generation: Signal::new(0),
            selected: Signal::new(BTreeSet::new()),
            anchor: Signal::new(None),
            view: Signal::new(ViewMode::Grid),
            sort: Signal::new(Sort::default()),
            renaming: Signal::new(None),
            menu: Signal::new(None),
            preview: Signal::new(None),
            editor: Signal::new(None),
            picker: Signal::new(None),
            new_folder: Signal::new(false),
            drag: Signal::new(None),
            drop_target: Signal::new(None),
            ext_drag: Signal::new(0),
            select_mode: Signal::new(false),
            touch: Signal::new(false),
            query: Signal::new(String::new()),
            results: Signal::new(None),
            searching: Signal::new(false),
            search_gen: Signal::new(0),
            toasts: Signal::new(Vec::new()),
            uploads: Signal::new(Vec::new()),
            roots: Signal::new(Vec::new()),
            reveal: Signal::new(None),
            history: Signal::new(Vec::new()),
            hist_idx: Signal::new(0),
            favs: Signal::new(None),
            home_dirs: Signal::new(Vec::new()),
            ext_dirs: Signal::new(Vec::new()),
            net_dirs: Signal::new(Vec::new()),
            device: Signal::new(String::new()),
            sidebar_open: Signal::new(false),
        }
    }

    /// Client-only setup after mount: stored view/sort, touch detection, quick access roots.
    pub fn init_client(mut self) {
        if let Some(v) = web::store_get(KEY_VIEW).and_then(|v| ViewMode::parse(&v)) {
            self.view.set(v);
        }
        if let Some(s) = web::store_get(KEY_SORT).and_then(|v| Sort::parse(&v)) {
            self.sort.set(s);
        }
        if let Some(f) = web::store_get(KEY_FAVS) {
            self.favs.set(Some(f.lines().filter(|l| l.starts_with('/')).map(str::to_string).collect()));
        }
        self.touch.set(web::is_touch());
        web::focus("fm-root");
        self.load_sidebar();
        self.spawn(async move {
            if let Ok(name) = device_name().await {
                self.device.set(name);
            }
        });
    }

    /// Loads the locations of the sidebar (top level, /Home, drives, network shares).
    pub fn load_sidebar(mut self) {
        self.spawn(async move {
            let Ok(l) = list_dir("/".into(), None).await else { return };
            let roots: Vec<FileEntry> = l.files.into_iter().filter(|f| f.is_dir()).collect();
            let has = |p: &str| roots.iter().any(|r| r.path == p);
            let (h, x, n) = (has("/Home"), has("/External"), has("/Network"));
            if *self.roots.peek() != roots {
                self.roots.set(roots);
            }
            async fn dirs(p: &str, on: bool) -> Vec<FileEntry> {
                if !on {
                    return Vec::new();
                }
                let mut v: Vec<FileEntry> = match list_dir(p.into(), None).await {
                    Ok(l) => l.files.into_iter().filter(|f| f.is_dir()).collect(),
                    Err(_) => Vec::new(),
                };
                logic::sort_entries(&mut v, Sort::default());
                v
            }
            let home = dirs("/Home", h).await;
            let ext = dirs("/External", x).await;
            let net = dirs("/Network", n).await;
            if *self.home_dirs.peek() != home {
                self.home_dirs.set(home);
            }
            if *self.ext_dirs.peek() != ext {
                self.ext_dirs.set(ext);
            }
            if *self.net_dirs.peek() != net {
                self.net_dirs.set(net);
            }
        });
    }

    // ---------- history ----------

    fn record_history(mut self, path: &str) {
        let mut h = self.history.write();
        let i = *self.hist_idx.peek();
        if h.get(i).map(String::as_str) == Some(path) {
            return;
        }
        if i > 0 && h.get(i - 1).map(String::as_str) == Some(path) {
            drop(h);
            self.hist_idx.set(i - 1);
        } else if h.get(i + 1).map(String::as_str) == Some(path) {
            drop(h);
            self.hist_idx.set(i + 1);
        } else {
            if !h.is_empty() {
                h.truncate(i + 1);
            }
            h.push(path.to_string());
            let n = h.len() - 1;
            drop(h);
            self.hist_idx.set(n);
        }
    }

    pub fn can_go(self, delta: i64) -> bool {
        let i = *self.hist_idx.read() as i64 + delta;
        i >= 0 && (i as usize) < self.history.read().len()
    }

    pub fn go(self, delta: i64) {
        let i = *self.hist_idx.peek() as i64 + delta;
        if i < 0 {
            return;
        }
        let target = self.history.peek().get(i as usize).cloned();
        if let Some(p) = target {
            self.navigate(&p);
        }
    }

    // ---------- favourites ----------

    pub fn fav_list(self) -> Vec<String> {
        match &*self.favs.read() {
            Some(f) => f.clone(),
            None => {
                let home = self.home_dirs.read();
                DEFAULT_FAVS.iter().filter(|p| home.iter().any(|d| d.path == **p)).map(|p| p.to_string()).collect()
            }
        }
    }

    pub fn is_fav(self, path: &str) -> bool {
        self.fav_list().iter().any(|p| p == path)
    }

    fn set_favs(mut self, list: Vec<String>) {
        web::store_set(KEY_FAVS, &list.join("\n"));
        self.favs.set(Some(list));
    }

    pub fn add_favs(self, paths: Vec<String>) {
        let mut list = self.fav_list();
        let mut added = 0;
        for p in paths {
            if logic::is_target_dir(&p) && !list.contains(&p) {
                list.push(p);
                added += 1;
            }
        }
        if added > 0 {
            self.set_favs(list);
            self.toast(if added == 1 { "Zu Favoriten hinzugefügt".to_string() } else { format!("{added} Ordner zu Favoriten hinzugefügt") });
        }
    }

    pub fn remove_fav(self, path: &str) {
        let mut list = self.fav_list();
        list.retain(|p| p != path);
        self.set_favs(list);
    }

    /// Keeps favourites in sync after `paths` were moved into `dir` (or trashed when `dir` is `None`).
    fn update_favs(self, paths: &[String], dir: Option<&str>) {
        let old = self.fav_list();
        let list = logic::remap_paths(&old, paths, dir);
        if list != old {
            self.set_favs(list);
        }
    }

    pub fn toggle_fav(self, path: &str) {
        if self.is_fav(path) { self.remove_fav(path) } else { self.add_favs(vec![path.to_string()]) }
    }

    // ---------- recent files ----------

    fn recent_list() -> Vec<FileEntry> {
        web::store_get(KEY_RECENT).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
    }

    fn store_recent(list: &[FileEntry]) {
        if let Ok(s) = serde_json::to_string(list) {
            web::store_set(KEY_RECENT, &s);
        }
    }

    /// Remembers an opened file for "Kürzlich" (newest first).
    pub fn remember(self, entry: &FileEntry) {
        let mut list = Self::recent_list();
        list.retain(|e| e.path != entry.path);
        list.insert(0, entry.clone());
        list.truncate(RECENT_MAX);
        Self::store_recent(&list);
    }

    /// Drops/renames entries of "Kürzlich" after trash/move/rename (`new` = new path or `None`).
    fn forget(self, paths: &[String], new: Option<&str>) {
        let mut list = Self::recent_list();
        let before = list.clone();
        list.retain_mut(|e| {
            if !paths.contains(&e.path) {
                return true;
            }
            match new {
                Some(n) => {
                    e.path = n.to_string();
                    e.name = logic::name_of(n);
                    true
                }
                None => false,
            }
        });
        if list != before {
            Self::store_recent(&list);
        }
    }

    #[cfg_attr(not(feature = "web"), allow(dead_code))]
    pub fn scope(self) -> ScopeId {
        self.scope
    }

    /// Spawns on the page scope, so the task survives closing a menu/dialog that started it.
    pub fn spawn(self, fut: impl std::future::Future<Output = ()> + 'static) {
        Runtime::current().spawn(self.scope, fut);
    }

    // ---------- loading ----------

    /// Reloads all loaded pages of the current folder (polling and after own actions).
    pub fn reload(mut self) {
        let generation = *self.generation.peek() + 1;
        self.generation.set(generation);
        let path = self.path.peek().clone();
        if path == RECENT {
            let list = Self::recent_list();
            if *self.entries.peek() != list {
                self.entries.set(list);
            }
            self.loaded.set(true);
            return;
        }
        let pages = *self.pages.peek();
        self.spawn(async move {
            let mut all: Vec<FileEntry> = Vec::new();
            let mut last: Option<String> = None;
            let mut more = false;
            let mut failure = None;
            for _ in 0..pages.max(1) {
                match list_dir(path.clone(), last.clone()).await {
                    Ok(l) => {
                        more = l.has_more;
                        last = l.files.last().map(|f| f.name.clone());
                        all.extend(l.files);
                        if !more {
                            break;
                        }
                    }
                    Err(e) => {
                        failure = Some(err_msg(&e));
                        break;
                    }
                }
            }
            if *self.generation.peek() != generation {
                return;
            }
            match failure {
                Some(e) if all.is_empty() => self.error.set(Some(e)),
                _ => {
                    self.error.set(None);
                    if self.results.peek().is_none() {
                        let keep: BTreeSet<String> = all.iter().map(|e| e.path.clone()).collect();
                        let pruned: BTreeSet<String> = self.selected.peek().intersection(&keep).cloned().collect();
                        if pruned.len() != self.selected.peek().len() {
                            self.selected.set(pruned);
                        }
                    }
                    if *self.entries.peek() != all {
                        self.entries.set(all);
                    }
                    if *self.has_more.peek() != more {
                        self.has_more.set(more);
                    }
                }
            }
            if !*self.loaded.peek() {
                self.loaded.set(true);
            }
        });
    }

    pub fn load_more(mut self) {
        if !*self.has_more.peek() || *self.loading_more.peek() || self.results.peek().is_some() {
            return;
        }
        let Some(last) = self.entries.peek().last().map(|e| e.name.clone()) else { return };
        self.loading_more.set(true);
        let path = self.path.peek().clone();
        let generation = *self.generation.peek();
        self.spawn(async move {
            let r = list_dir(path, Some(last)).await;
            self.loading_more.set(false);
            if *self.generation.peek() != generation {
                return;
            }
            match r {
                Ok(l) => {
                    self.has_more.set(l.has_more);
                    let mut entries = self.entries.write();
                    for f in l.files {
                        if !entries.iter().any(|e| e.path == f.path) {
                            entries.push(f);
                        }
                    }
                    drop(entries);
                    *self.pages.write() += 1;
                }
                Err(e) => self.toast_err(err_msg(&e)),
            }
        });
    }

    /// Called when the route's folder changes.
    pub fn enter(mut self, path: String) {
        self.record_history(&path);
        self.sidebar_open.set(false);
        self.path.set(path);
        self.entries.set(Vec::new());
        self.loaded.set(false);
        self.error.set(None);
        self.pages.set(1);
        self.has_more.set(false);
        self.renaming.set(None);
        self.menu.set(None);
        self.preview.set(None);
        self.query.set(String::new());
        self.results.set(None);
        let reveal = self.reveal.take();
        self.anchor.set(reveal.clone());
        self.selected.set(reveal.into_iter().collect());
        self.reload();
    }

    pub fn navigate(self, path: &str) {
        navigator().push(Route::files(path));
    }

    /// Display list: search results or the folder, sorted.
    pub fn visible(self) -> Vec<FileEntry> {
        let sort = *self.sort.read();
        let mut list = match &*self.results.read() {
            Some(Ok(r)) => r.clone(),
            Some(Err(_)) => Vec::new(),
            None => self.entries.read().clone(),
        };
        logic::sort_entries(&mut list, sort);
        list
    }

    pub fn find(self, path: &str) -> Option<FileEntry> {
        let in_results = self.results.peek().as_ref().and_then(|r| r.as_ref().ok()).and_then(|r| r.iter().find(|e| e.path == path).cloned());
        in_results.or_else(|| self.entries.peek().iter().find(|e| e.path == path).cloned())
    }

    // ---------- search ----------

    /// Debounced search in /Home.
    pub fn search(mut self, q: String) {
        self.query.set(q.clone());
        let generation = *self.search_gen.peek() + 1;
        self.search_gen.set(generation);
        if q.trim().is_empty() {
            self.results.set(None);
            self.searching.set(false);
            return;
        }
        self.searching.set(true);
        self.spawn(async move {
            crate::utils::sleep_ms(350).await;
            if *self.search_gen.peek() != generation {
                return;
            }
            let r = search_files(q.trim().to_string()).await.map_err(|e| err_msg(&e));
            if *self.search_gen.peek() == generation {
                self.selected.set(BTreeSet::new());
                self.results.set(Some(r));
                self.searching.set(false);
            }
        });
    }

    pub fn clear_search(mut self) {
        *self.search_gen.write() += 1;
        self.query.set(String::new());
        self.results.set(None);
        self.searching.set(false);
        self.selected.set(BTreeSet::new());
    }

    // ---------- selection ----------

    pub fn is_selected(self, path: &str) -> bool {
        self.selected.read().contains(path)
    }

    pub fn select_only(mut self, path: &str) {
        self.selected.set(BTreeSet::from([path.to_string()]));
        self.anchor.set(Some(path.to_string()));
    }

    pub fn toggle(mut self, path: &str) {
        let mut sel = self.selected.write();
        if !sel.remove(path) {
            sel.insert(path.to_string());
        }
        drop(sel);
        self.anchor.set(Some(path.to_string()));
    }

    pub fn select_all(mut self) {
        let all: BTreeSet<String> = self.visible().into_iter().map(|e| e.path).collect();
        self.selected.set(all);
    }

    pub fn clear_selection(mut self) {
        if !self.selected.peek().is_empty() {
            self.selected.set(BTreeSet::new());
        }
    }

    pub fn selection(self) -> Vec<String> {
        self.selected.peek().iter().cloned().collect()
    }

    /// Click on an entry: Finder-like selection on desktop, open/toggle on touch.
    pub fn click(mut self, entry: &FileEntry, toggle_mod: bool, shift: bool) {
        let path = entry.path.clone();
        let selecting = *self.select_mode.peek();
        if selecting {
            self.toggle(&path);
        } else if *self.touch.peek() {
            self.open(entry);
        } else if shift {
            let order: Vec<String> = self.visible().into_iter().map(|e| e.path).collect();
            let anchor = self.anchor.peek().clone();
            let r = logic::range(&order, anchor.as_deref(), &path);
            let mut sel: BTreeSet<String> = if toggle_mod { self.selected.peek().clone() } else { BTreeSet::new() };
            sel.extend(r);
            self.selected.set(sel);
        } else if toggle_mod {
            self.toggle(&path);
        } else {
            self.select_only(&path);
        }
    }

    /// Moves the single selection up/down in display order (arrow keys).
    pub fn step(self, delta: i64) {
        let order: Vec<String> = self.visible().into_iter().map(|e| e.path).collect();
        if order.is_empty() {
            return;
        }
        let cur = self.anchor.peek().clone().and_then(|a| order.iter().position(|p| *p == a));
        let next = match cur {
            Some(i) => (i as i64 + delta).clamp(0, order.len() as i64 - 1) as usize,
            None => 0,
        };
        self.select_only(&order[next]);
    }

    // ---------- open / preview ----------

    pub fn open(mut self, entry: &FileEntry) {
        if entry.is_dir() {
            self.navigate(&entry.path);
        } else if logic::editable(logic::kind_of(entry)) {
            self.remember(entry);
            self.editor.set(Some(entry.path.clone()));
        } else if logic::previewable(logic::kind_of(entry)) {
            self.remember(entry);
            self.preview.set(Some(entry.path.clone()));
        } else {
            self.remember(entry);
            web::trigger_download(&download_url(&entry.path));
        }
    }

    /// Files the preview can page through (display order).
    pub fn preview_list(self) -> Vec<FileEntry> {
        self.visible().into_iter().filter(|e| logic::previewable(logic::kind_of(e))).collect()
    }

    pub fn preview_step(mut self, delta: i64) {
        let list = self.preview_list();
        let Some(cur) = self.preview.peek().clone() else { return };
        if let Some(i) = list.iter().position(|e| e.path == cur) {
            let n = list.len() as i64;
            let j = ((i as i64 + delta) % n + n) % n;
            let p = list[j as usize].path.clone();
            self.select_only(&p);
            self.preview.set(Some(p));
        }
    }

    // ---------- notifications ----------

    pub fn toast(self, text: impl Into<String>) {
        self.push_toast(text.into(), false);
    }

    pub fn toast_err(self, text: impl Into<String>) {
        self.push_toast(text.into(), true);
    }

    fn push_toast(mut self, text: String, error: bool) {
        let id = self.toasts.peek().iter().map(|t| t.id).max().unwrap_or(0) + 1;
        self.toasts.write().push(Toast { id, text, error });
        let mut toasts = self.toasts;
        self.spawn(async move {
            crate::utils::sleep_ms(if error { 9000 } else { 4000 }).await;
            toasts.write().retain(|t| t.id != id);
        });
    }

    // ---------- file operations ----------

    /// Runs `move`/`copy`/`trash` for several paths, reports and reloads.
    pub fn batch(self, op: &'static str, paths: Vec<String>, arg: String) {
        if paths.is_empty() {
            return;
        }
        let n = paths.len();
        self.spawn(async move {
            let moved = paths.clone();
            match fs_batch(op.to_string(), paths, arg.clone()).await {
                Ok((ok, errors)) => {
                    if ok > 0 {
                        let what = if ok == 1 { "1 Element".to_string() } else { format!("{ok} Elemente") };
                        let target = logic::name_of(&arg);
                        self.toast(match op {
                            "move" => format!("{what} nach „{target}“ verschoben"),
                            "copy" => format!("{what} nach „{target}“ kopiert"),
                            _ => format!("{what} in den Papierkorb gelegt"),
                        });
                    }
                    for e in errors.iter().take(5) {
                        self.toast_err(e.clone());
                    }
                    if errors.len() > 5 {
                        self.toast_err(format!("… und {} weitere Fehler", errors.len() - 5));
                    }
                    if ok == n {
                        self.clear_selection();
                        if op != "copy" {
                            self.forget(&moved, None);
                            self.update_favs(&moved, (op == "move").then_some(arg.as_str()));
                        }
                    }
                }
                Err(e) => self.toast_err(err_msg(&e)),
            }
            self.reload();
            if self.results.peek().is_some() {
                let q = self.query.peek().clone();
                self.search(q);
            }
        });
    }

    pub fn trash(self, paths: Vec<String>) {
        self.batch("trash", paths, String::new());
    }

    pub fn move_to(self, paths: Vec<String>, dir: &str) {
        if !logic::can_move_into(&paths, dir) {
            return;
        }
        self.batch("move", paths, dir.to_string());
    }

    pub fn rename(mut self, path: String, new_name: String) {
        self.renaming.set(None);
        let old = logic::name_of(&path);
        let name = match logic::validate_name(&new_name) {
            Ok(n) if n == old => return,
            Ok(n) => n,
            Err(e) => {
                self.toast_err(e);
                return;
            }
        };
        self.spawn(async move {
            match fs_op("rename".into(), path.clone(), name.clone()).await {
                Ok(_) => {
                    let new_path = logic::join(&logic::parent(&path), &name);
                    self.forget(std::slice::from_ref(&path), Some(&new_path));
                    if self.is_fav(&path) {
                        let list = self.fav_list().into_iter().map(|p| if p == path { new_path.clone() } else { p }).collect();
                        self.set_favs(list);
                    }
                    self.select_only(&new_path);
                    self.reload();
                }
                Err(e) => self.toast_err(err_msg(&e)),
            }
        });
    }

    pub fn mkdir(mut self, name: String) {
        let dir = self.path.peek().clone();
        let name = match logic::validate_name(&name) {
            Ok(n) => n,
            Err(e) => {
                self.toast_err(e);
                return;
            }
        };
        self.new_folder.set(false);
        self.spawn(async move {
            match fs_op("mkdir".into(), dir.clone(), name.clone()).await {
                Ok(_) => {
                    self.select_only(&logic::join(&dir, &name));
                    self.reload();
                }
                Err(e) => self.toast_err(err_msg(&e)),
            }
        });
    }

    pub fn download(self, paths: Vec<String>) {
        let (dirs, files): (Vec<String>, Vec<String>) = paths.into_iter().partition(|p| self.find(p).is_some_and(|e| e.is_dir()));
        let skipped = dirs.len();
        if files.is_empty() {
            self.toast_err("Ordner können nicht als Ganzes heruntergeladen werden");
            return;
        }
        if skipped > 0 {
            self.toast("Ordner werden beim Herunterladen übersprungen");
        }
        self.spawn(async move {
            for (i, p) in files.iter().enumerate() {
                if i > 0 {
                    crate::utils::sleep_ms(400).await;
                }
                web::trigger_download(&download_url(p));
            }
        });
    }

    pub fn copy_path(self, path: &str) {
        web::copy_text(path);
        self.toast("Pfad kopiert");
    }

    /// Opens the folder of a search hit and selects it.
    pub fn reveal_in_folder(mut self, path: &str) {
        self.reveal.set(Some(path.to_string()));
        let dir = logic::parent(path);
        if *self.path.peek() == dir {
            self.enter(dir);
        } else {
            self.navigate(&dir);
        }
    }

    // ---------- drag & drop ----------

    pub fn start_drag(mut self, path: &str) {
        let paths = if self.selected.peek().contains(path) { self.selection() } else { vec![path.to_string()] };
        if !self.selected.peek().contains(path) {
            self.select_only(path);
        }
        self.drag.set(Some(paths));
    }

    pub fn end_drag(mut self) {
        self.drag.set(None);
        self.drop_target.set(None);
    }

    /// Whether the current drag may be dropped on `dir`.
    pub fn can_drop(self, dir: &str, e: &DragEvent) -> bool {
        match &*self.drag.peek() {
            Some(paths) => logic::can_move_into(paths, dir),
            None => logic::is_target_dir(dir) && web::drag_has_files(e),
        }
    }

    pub fn drag_over(mut self, dir: &str, e: &DragEvent) {
        if self.can_drop(dir, e) {
            e.prevent_default();
            e.stop_propagation();
            web::set_drop_effect(e, if self.drag.peek().is_some() { "move" } else { "copy" });
            if self.drop_target.peek().as_deref() != Some(dir) {
                self.drop_target.set(Some(dir.to_string()));
            }
        }
    }

    pub fn drag_leave(mut self, dir: &str) {
        if self.drop_target.peek().as_deref() == Some(dir) {
            self.drop_target.set(None);
        }
    }

    /// Drop on a folder: in-app drag = move, OS files = upload into it.
    pub fn drop_on(mut self, dir: &str, e: &DragEvent) {
        e.prevent_default();
        e.stop_propagation();
        self.ext_drag.set(0);
        self.drop_target.set(None);
        if let Some(paths) = self.drag.take() {
            self.move_to(paths, dir);
        } else if logic::is_target_dir(dir) {
            super::upload::from_drop(self, e, dir.to_string());
        }
    }
}
