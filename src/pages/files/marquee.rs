//! Rubber-band selection: drag on empty space in the content panel to select files and folders.
//! Ctrl/Cmd/Shift add to the current selection; the panel scrolls when the pointer nears an edge.

use std::collections::BTreeSet;

use dioxus::prelude::*;

use super::state::Fm;

pub const SCROLL_ID: &str = "fm-scroll";

/// Item rectangle in content coordinates (inside the scroll container).
type ItemRect = (String, f64, f64, f64, f64);

#[derive(Clone, PartialEq, Default)]
pub struct Marquee {
    start: (f64, f64),
    cur: (f64, f64),
    client: (f64, f64),
    rects: Vec<ItemRect>,
    base: BTreeSet<String>,
    additive: bool,
    active: bool,
}

impl Marquee {
    fn bounds(&self) -> (f64, f64, f64, f64) {
        let (x1, y1) = self.start;
        let (x2, y2) = self.cur;
        (x1.min(x2), y1.min(y2), x1.max(x2), y1.max(y2))
    }

    #[cfg_attr(not(feature = "web"), allow(dead_code))]
    fn hits(&self) -> BTreeSet<String> {
        let (l, t, r, b) = self.bounds();
        self.rects
            .iter()
            .filter(|(_, il, it, ir, ib)| *il < r && *ir > l && *it < b && *ib > t)
            .map(|(p, ..)| p.clone())
            .collect()
    }
}

#[cfg(feature = "web")]
mod dom {
    use wasm_bindgen::JsCast;

    pub fn container() -> Option<web_sys::Element> {
        web_sys::window()?.document()?.get_element_by_id(super::SCROLL_ID)
    }

    /// Client → content coordinates of the scroll container.
    pub fn to_content(c: &web_sys::Element, x: f64, y: f64) -> (f64, f64) {
        let r = c.get_bounding_client_rect();
        (x - r.left() + f64::from(c.scroll_left()), y - r.top() + f64::from(c.scroll_top()))
    }

    pub fn item_rects(c: &web_sys::Element) -> Vec<super::ItemRect> {
        let mut out = Vec::new();
        let Ok(list) = c.query_selector_all("[data-path]") else { return out };
        let cr = c.get_bounding_client_rect();
        let (sx, sy) = (f64::from(c.scroll_left()), f64::from(c.scroll_top()));
        for i in 0..list.length() {
            let Some(el) = list.item(i).and_then(|n| n.dyn_into::<web_sys::Element>().ok()) else { continue };
            let Some(path) = el.get_attribute("data-path") else { continue };
            let r = el.get_bounding_client_rect();
            let l = r.left() - cr.left() + sx;
            let t = r.top() - cr.top() + sy;
            out.push((path, l, t, l + r.width(), t + r.height()));
        }
        out
    }

    /// Pointer went down on empty space (not on an item, button, input …) and not on the scrollbar.
    pub fn on_background(e: &web_sys::PointerEvent, c: &web_sys::Element) -> bool {
        let Some(target) = e.target().and_then(|t| t.dyn_into::<web_sys::Element>().ok()) else { return false };
        if target.closest("[data-path],button,a,input,textarea,select,label,[role=menu]").ok().flatten().is_some() {
            return false;
        }
        let r = c.get_bounding_client_rect();
        f64::from(e.client_x()) < r.right() - 16.0
    }

    /// Scrolls the container when the pointer is near its top/bottom edge.
    pub fn autoscroll(c: &web_sys::Element, y: f64) {
        let r = c.get_bounding_client_rect();
        if y < r.top() + 32.0 {
            c.set_scroll_top(c.scroll_top() - 18);
        } else if y > r.bottom() - 32.0 {
            c.set_scroll_top(c.scroll_top() + 18);
        }
    }
}

/// `onpointerdown` of the scroll container.
pub fn begin(e: &PointerEvent, fm: Fm, mut state: Signal<Option<Marquee>>) {
    #[cfg(feature = "web")]
    {
        use dioxus::html::input_data::MouseButton;
        if e.pointer_type() != "mouse" || *fm.touch.peek() || e.trigger_button() != Some(MouseButton::Primary) {
            return;
        }
        let Some(raw) = e.data().downcast::<web_sys::PointerEvent>().cloned() else { return };
        let Some(c) = dom::container() else { return };
        if !dom::on_background(&raw, &c) {
            return;
        }
        let p = e.client_coordinates();
        let start = dom::to_content(&c, p.x, p.y);
        let m = e.modifiers();
        let additive = m.ctrl() || m.meta() || m.shift();
        state.set(Some(Marquee {
            start,
            cur: start,
            client: (p.x, p.y),
            rects: dom::item_rects(&c),
            base: if additive { fm.selected.peek().clone() } else { BTreeSet::new() },
            additive,
            active: false,
        }));
    }
    #[cfg(not(feature = "web"))]
    {
        let _ = (e, fm, &mut state);
    }
}

fn update(fm: Fm, mut state: Signal<Option<Marquee>>, x: f64, y: f64) {
    #[cfg(feature = "web")]
    {
        let Some(mut m) = state.peek().clone() else { return };
        let Some(c) = dom::container() else { return };
        dom::autoscroll(&c, y);
        m.client = (x, y);
        m.cur = dom::to_content(&c, x, y);
        if !m.active && (m.cur.0 - m.start.0).abs().max((m.cur.1 - m.start.1).abs()) < 5.0 {
            return;
        }
        m.active = true;
        let hits = m.hits();
        let next: BTreeSet<String> = if m.additive { m.base.union(&hits).cloned().collect() } else { hits };
        let mut sel = fm.selected;
        if *sel.peek() != next {
            sel.set(next);
        }
        state.set(Some(m));
    }
    #[cfg(not(feature = "web"))]
    {
        let _ = (fm, &mut state, x, y);
    }
}

fn finish(fm: Fm, mut state: Signal<Option<Marquee>>) {
    let m = state.peek().clone();
    state.set(None);
    if let Some(m) = m {
        // A plain click on empty space clears the selection (like a file manager).
        if !m.active && !m.additive && !*fm.select_mode.peek() {
            fm.clear_selection();
        }
    }
}

/// The selection rectangle, rendered inside the scroll container (content coordinates).
#[component]
pub fn MarqueeRect(state: Signal<Option<Marquee>>) -> Element {
    let Some(m) = state.read().clone().filter(|m| m.active) else { return rsx! {} };
    let (l, t, r, b) = m.bounds();
    rsx! {
        div {
            class: "pointer-events-none absolute z-20 rounded-md border border-[#2f8f99] bg-[#2f8f99]/15",
            style: "left: {l}px; top: {t}px; width: {r - l}px; height: {b - t}px;",
        }
    }
}

/// Full-screen layer that captures the pointer while a marquee is being drawn.
#[component]
pub fn MarqueeCapture(state: Signal<Option<Marquee>>) -> Element {
    let fm = use_context::<Fm>();
    if state.read().is_none() {
        return rsx! {};
    }
    rsx! {
        div {
            class: "fixed inset-0 z-[55] cursor-default select-none",
            onpointermove: move |e: PointerEvent| {
                let p = e.client_coordinates();
                update(fm, state, p.x, p.y);
            },
            onpointerup: move |_| finish(fm, state),
            onpointercancel: move |_| finish(fm, state),
            onwheel: move |e: WheelEvent| {
                // keep scrolling possible while selecting
                #[cfg(feature = "web")]
                if let Some(c) = dom::container() {
                    c.set_scroll_top(c.scroll_top() + e.delta().strip_units().y as i32);
                    let client = state.peek().as_ref().map(|m| m.client);
                    if let Some((x, y)) = client {
                        update(fm, state, x, y);
                    }
                }
                #[cfg(not(feature = "web"))]
                let _ = e;
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hits_items_inside_rectangle_in_any_direction() {
        let rects = vec![
            ("/a".to_string(), 0.0, 0.0, 100.0, 100.0),
            ("/b".to_string(), 120.0, 0.0, 220.0, 100.0),
            ("/c".to_string(), 0.0, 120.0, 100.0, 220.0),
        ];
        // dragged from bottom-right to top-left, touching /b and /c only partially
        let m = Marquee { start: (150.0, 150.0), cur: (90.0, 50.0), rects, ..Default::default() };
        let hits: Vec<String> = m.hits().into_iter().collect();
        assert_eq!(hits, vec!["/a", "/b", "/c"]);
        let small = Marquee { start: (130.0, 10.0), cur: (140.0, 20.0), rects: m.rects.clone(), ..Default::default() };
        assert_eq!(small.hits().into_iter().collect::<Vec<_>>(), vec!["/b"]);
    }
}
