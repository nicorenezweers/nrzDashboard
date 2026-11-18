//! Grid layout engine: place a moved/resized widget, push colliding widgets down, then compact upwards
//! (vertical gravity like react-grid-layout, which Homarr uses).

use crate::api::board::{COLS, Widget};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub const fn of(w: &Widget) -> Self {
        Self { x: w.x, y: w.y, w: w.w, h: w.h }
    }

    pub const fn overlaps(&self, o: &Self) -> bool {
        self.x < o.x + o.w && o.x < self.x + self.w && self.y < o.y + o.h && o.y < self.y + self.h
    }
}

fn collides(r: &Rect, placed: &[Rect]) -> bool {
    placed.iter().any(|p| r.overlaps(p))
}

/// Returns a new layout where `id` sits at `target` (clamped) and nothing overlaps.
pub fn place(widgets: &[Widget], id: &str, target: Rect) -> Vec<Widget> {
    let mut out: Vec<Widget> = widgets.to_vec();
    let Some(idx) = out.iter().position(|w| w.id == id) else { return out };
    let (mw, mh) = out[idx].kind.min_size();
    let w = target.w.clamp(mw, COLS);
    let h = target.h.clamp(mh, 24);
    let moved = Rect { x: target.x.clamp(0, COLS - w), y: target.y.max(0), w, h };
    out[idx].x = moved.x;
    out[idx].y = moved.y;
    out[idx].w = moved.w;
    out[idx].h = moved.h;

    // Moved widget first, then the rest top-to-bottom; push down until free.
    let mut order: Vec<usize> = (0..out.len()).filter(|&i| i != idx).collect();
    order.sort_by_key(|&i| (out[i].y, out[i].x));
    let mut placed = vec![moved];
    for i in order {
        let mut r = Rect::of(&out[i]);
        while collides(&r, &placed) {
            r.y += 1;
        }
        out[i].y = r.y;
        placed.push(r);
    }
    compact(&mut out, Some(id));
    out
}

/// Moves every widget up as far as possible. The pinned widget keeps its row unless the space
/// above it is free (so dropping into a gap works but it never jumps over others).
pub fn compact(widgets: &mut [Widget], _pinned: Option<&str>) {
    let mut order: Vec<usize> = (0..widgets.len()).collect();
    order.sort_by_key(|&i| (widgets[i].y, widgets[i].x));
    let mut placed: Vec<Rect> = Vec::new();
    for i in order {
        let mut r = Rect::of(&widgets[i]);
        while r.y > 0 && !collides(&Rect { y: r.y - 1, ..r }, &placed) {
            r.y -= 1;
        }
        // Defensive: never overlap (could happen with corrupt input).
        while collides(&r, &placed) {
            r.y += 1;
        }
        widgets[i].y = r.y;
        placed.push(r);
    }
}

/// First free spot for a new widget of size (w, h): scan rows top-down, columns left-right.
pub fn free_spot(widgets: &[Widget], w: i32, h: i32) -> (i32, i32) {
    let placed: Vec<Rect> = widgets.iter().map(Rect::of).collect();
    let w = w.min(COLS);
    for y in 0..1000 {
        for x in 0..=(COLS - w) {
            let r = Rect { x, y, w, h };
            if !collides(&r, &placed) {
                return (x, y);
            }
        }
    }
    (0, placed.iter().map(|r| r.y + r.h).max().unwrap_or(0))
}

/// Number of rows the layout occupies.
pub fn rows(widgets: &[Widget]) -> i32 {
    widgets.iter().map(|w| w.y + w.h).max().unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::board::WidgetKind;
    use serde_json::Value;

    fn w(id: &str, x: i32, y: i32, ww: i32, h: i32) -> Widget {
        Widget { id: id.into(), kind: WidgetKind::Notes, x, y, w: ww, h, title: String::new(), options: Value::Null }
    }

    fn no_overlaps(ws: &[Widget]) -> bool {
        ws.iter().enumerate().all(|(i, a)| ws.iter().skip(i + 1).all(|b| !Rect::of(a).overlaps(&Rect::of(b))))
    }

    #[test]
    fn moving_onto_other_pushes_it_down() {
        let ws = vec![w("a", 0, 0, 4, 2), w("b", 4, 0, 4, 2)];
        let out = place(&ws, "b", Rect { x: 0, y: 0, w: 4, h: 2 });
        assert!(no_overlaps(&out));
        let b = out.iter().find(|x| x.id == "b").unwrap();
        let a = out.iter().find(|x| x.id == "a").unwrap();
        assert_eq!((b.x, b.y), (0, 0));
        assert_eq!((a.x, a.y), (0, 2));
    }

    #[test]
    fn compacts_upwards() {
        let mut ws = vec![w("a", 0, 5, 4, 2), w("b", 4, 3, 4, 2)];
        compact(&mut ws, None);
        assert_eq!(ws[0].y, 0);
        assert_eq!(ws[1].y, 0);
    }

    #[test]
    fn resize_respects_bounds_and_min() {
        let ws = vec![w("a", 10, 0, 2, 2)];
        let out = place(&ws, "a", Rect { x: 10, y: 0, w: 6, h: 0 });
        assert_eq!((out[0].x, out[0].w, out[0].h), (6, 6, 2));
    }

    #[test]
    fn free_spot_finds_gap() {
        let ws = vec![w("a", 0, 0, 6, 2)];
        assert_eq!(free_spot(&ws, 6, 2), (6, 0));
        assert_eq!(free_spot(&ws, 8, 1), (0, 2));
    }

    #[test]
    fn many_moves_never_overlap() {
        let mut ws = vec![w("a", 0, 0, 4, 2), w("b", 4, 0, 4, 3), w("c", 8, 0, 4, 1), w("d", 0, 2, 12, 1)];
        for (id, x, y) in [("d", 3, 0), ("a", 8, 1), ("c", 0, 0), ("b", 6, 4)] {
            let cur = ws.iter().find(|x| x.id == id).unwrap().clone();
            ws = place(&ws, id, Rect { x, y, w: cur.w, h: cur.h });
            assert!(no_overlaps(&ws), "{ws:?}");
        }
    }
}
