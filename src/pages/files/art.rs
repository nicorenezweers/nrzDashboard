//! Hand-drawn icons in the style of the umbrelOS Files app: teal folder (with an emblem for well-known
//! folders) and a paper sheet with the file extension. The folder colour is deliberately fixed.

use dioxus::prelude::*;

use super::logic::{self, Kind};

const BACK: &str = "#17484e";
const FRONT: &str = "#1f5f66";
const EDGE: &str = "#4f9ba1";
const EMB_FILL: &str = "#174c52";
const EMB_LINE: &str = "#2e767d";
const EMB_DETAIL: &str = "#8cc3c7";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Emblem {
    None,
    Download,
    Document,
    Photo,
    Video,
    Music,
}

/// Emblem for well-known folder names (Downloads, Documents, Photos, Videos, Music).
pub fn emblem_for(name: &str) -> Emblem {
    let n = name.to_lowercase();
    match n.as_str() {
        "downloads" | "download" => Emblem::Download,
        "documents" | "dokumente" | "docs" => Emblem::Document,
        "photos" | "fotos" | "bilder" | "pictures" | "images" => Emblem::Photo,
        "videos" | "filme" | "movies" => Emblem::Video,
        "music" | "musik" | "nrzmusic" => Emblem::Music,
        _ => Emblem::None,
    }
}

fn emblem(e: Emblem) -> Element {
    let sw = "2.2";
    match e {
        Emblem::None => rsx! {},
        Emblem::Download => rsx! {
            circle { cx: "40", cy: "39", r: "9.5", fill: EMB_FILL, stroke: EMB_LINE, stroke_width: "1.4" }
            path { d: "M40 34v9.5M35.8 39.6l4.2 4.2 4.2-4.2", fill: "none", stroke: EMB_DETAIL, stroke_width: sw, stroke_linecap: "round", stroke_linejoin: "round" }
        },
        Emblem::Document => rsx! {
            path { d: "M35 28.5h7.5l5.5 5.5v14.5a1.5 1.5 0 0 1-1.5 1.5H35a1.5 1.5 0 0 1-1.5-1.5V30a1.5 1.5 0 0 1 1.5-1.5z", fill: EMB_FILL, stroke: EMB_LINE, stroke_width: "1.4", stroke_linejoin: "round" }
            path { d: "M42.5 28.5V34H48", fill: "none", stroke: EMB_LINE, stroke_width: "1.4", stroke_linejoin: "round" }
        },
        Emblem::Photo => rsx! {
            rect { x: "29.5", y: "30.5", width: "21", height: "17", rx: "3.5", fill: EMB_FILL, stroke: EMB_LINE, stroke_width: "1.4" }
            path { d: "M32 45l5.2-6 4 4 3.2-3.2 4 5", fill: "none", stroke: EMB_DETAIL, stroke_width: "1.8", stroke_linecap: "round", stroke_linejoin: "round" }
            circle { cx: "45", cy: "35", r: "1.7", fill: EMB_DETAIL }
        },
        Emblem::Video => rsx! {
            rect { x: "28.5", y: "31", width: "23", height: "16", rx: "2.5", fill: EMB_FILL, stroke: EMB_LINE, stroke_width: "1.4" }
            path { d: "M28.5 35h23M28.5 43h23M34 31v4M40 31v4M46 31v4M34 43v4M40 43v4M46 43v4", fill: "none", stroke: EMB_LINE, stroke_width: "1.2" }
        },
        Emblem::Music => rsx! {
            path { d: "M37.5 46V33.2l10-2.2v12", fill: "none", stroke: EMB_DETAIL, stroke_width: sw, stroke_linecap: "round", stroke_linejoin: "round" }
            circle { cx: "35", cy: "46", r: "2.8", fill: EMB_DETAIL }
            circle { cx: "45", cy: "43", r: "2.8", fill: EMB_DETAIL }
        },
    }
}

/// Teal folder; `class` sets the size (e.g. `w-20`).
pub fn folder_art(e: Emblem, class: &str) -> Element {
    rsx! {
        svg {
            class: "{class} shrink-0 drop-shadow-sm",
            view_box: "0 0 80 64",
            "aria-hidden": "true",
            path { d: "M6 12a5 5 0 0 1 5-5h17a5 5 0 0 1 3.6 1.5l3.8 4H69a5 5 0 0 1 5 5V54a5 5 0 0 1-5 5H11a5 5 0 0 1-5-5z", fill: BACK }
            rect { x: "13", y: "12.5", width: "52", height: "22", rx: "2", fill: "#b9c9cc" }
            rect { x: "17", y: "10.5", width: "50", height: "22", rx: "2", fill: "#eef3f4", transform: "rotate(2.2 42 21)" }
            rect { x: "6", y: "17", width: "68", height: "42", rx: "6", fill: FRONT }
            rect { x: "6", y: "40", width: "68", height: "19", rx: "6", fill: "#000", opacity: "0.08" }
            rect { x: "11", y: "17", width: "58", height: "1.3", rx: "0.65", fill: EDGE, opacity: "0.75" }
            {emblem(e)}
        }
    }
}

fn kind_color(k: Kind) -> &'static str {
    match k {
        Kind::Pdf => "#e5484d",
        Kind::Image => "#8e4ec6",
        Kind::Video => "#e54666",
        Kind::Audio => "#f76b15",
        Kind::Text => "#3e63dd",
        Kind::Code => "#12a594",
        Kind::Archive => "#ad7f58",
        Kind::Folder | Kind::Other => "#6f7a7c",
    }
}

/// Paper sheet with the extension as a coloured label (`PDF`, `ZIP`, …).
pub fn file_art(name: &str, class: &str) -> Element {
    let kind = logic::kind_by_name(name);
    let mut ext = logic::ext_of(name).to_uppercase();
    ext.truncate(4);
    let color = kind_color(kind);
    let label_w = 14.0 + ext.len() as f64 * 7.0;
    let label_x = 32.0 - label_w / 2.0;
    rsx! {
        svg {
            class: "{class} shrink-0 drop-shadow-sm",
            view_box: "0 0 64 80",
            "aria-hidden": "true",
            path { d: "M14 4h26l16 16v52a4 4 0 0 1-4 4H14a4 4 0 0 1-4-4V8a4 4 0 0 1 4-4z", fill: "#dfe6e8" }
            path { d: "M40 4v12a4 4 0 0 0 4 4h12z", fill: "#b3c0c3" }
            path { d: "M18 28h22M18 34h28M18 40h24", stroke: "#c3cdd0", stroke_width: "2.4", stroke_linecap: "round" }
            if !ext.is_empty() {
                rect { x: "{label_x}", y: "50", width: "{label_w}", height: "16", rx: "4", fill: color }
                text {
                    x: "32",
                    y: "61.5",
                    fill: "#fff",
                    font_size: "10.5",
                    font_weight: "700",
                    text_anchor: "middle",
                    font_family: "ui-sans-serif, system-ui, sans-serif",
                    "{ext}"
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emblems() {
        assert_eq!(emblem_for("Downloads"), Emblem::Download);
        assert_eq!(emblem_for("nrzMusic"), Emblem::Music);
        assert_eq!(emblem_for("Bilder"), Emblem::Photo);
        assert_eq!(emblem_for("Projekte"), Emblem::None);
    }
}
