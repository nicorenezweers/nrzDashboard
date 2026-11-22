//! Live values of the home screen. The first value comes with the page itself (the server writes
//! its cache into the HTML, see [`use_init`]), so nothing has to load when the dashboard opens.
//! Afterwards values are refreshed in the background without ever going back to a loading state –
//! the old value stays visible until the new one has arrived.

use std::future::Future;

use dioxus::prelude::*;

use crate::model::InitData;

pub struct Live<T: 'static> {
    value: Signal<Option<T>>,
    tick: Signal<u32>,
}

impl<T> Clone for Live<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for Live<T> {}

impl<T: 'static> Live<T> {
    /// Current value (`None` only until the very first load, when the server had nothing yet).
    pub fn read(&self) -> dioxus::signals::ReadableRef<'_, Signal<Option<T>>> {
        self.value.read()
    }

    /// Reloads in the background.
    pub fn restart(&mut self) {
        *self.tick.write() += 1;
    }
}

/// `refresh` reloads all live values together (e.g. after saving settings).
pub fn use_live<T, F>(refresh: Signal<u32>, initial: Option<T>, fetch: fn() -> F) -> Live<T>
where
    F: Future<Output = T> + 'static,
    T: 'static,
{
    let mut value = use_signal(|| initial);
    let tick = use_signal(|| 0u32);
    // Only the newest request may write (an older, slower answer is dropped).
    let mut seq = use_signal(|| 0u64);
    use_effect(move || {
        let (t, r) = (tick(), refresh());
        // The first run already has the value from the page.
        if t == 0 && r == 0 && value.peek().is_some() {
            return;
        }
        let mine = *seq.peek() + 1;
        seq.set(mine);
        spawn(async move {
            let v = fetch().await;
            if *seq.peek() == mine {
                value.set(Some(v));
            }
        });
    });
    Live { value, tick }
}

pub const INIT_ID: &str = "nrz-init";

/// Start data of the page: on the server straight from the cache, in the browser read back from
/// the `<script id="nrz-init">` the server wrote. Returns the raw JSON too – the browser renders
/// exactly the same text again so hydration matches.
pub fn use_init() -> (String, InitData) {
    use_hook(|| {
        #[cfg(feature = "server")]
        {
            let data = crate::server::cache::snapshot();
            // `</` must not end the script element early (`<\/` is the same in JSON).
            let raw = serde_json::to_string(&data).unwrap_or_default().replace("</", "<\\/");
            (raw, data)
        }
        #[cfg(not(feature = "server"))]
        {
            #[cfg(all(feature = "web", target_arch = "wasm32"))]
            let raw = web_sys::window()
                .and_then(|w| w.document())
                .and_then(|d| d.get_element_by_id(INIT_ID))
                .and_then(|e| e.text_content())
                .unwrap_or_default();
            #[cfg(not(all(feature = "web", target_arch = "wasm32")))]
            let raw = String::new();
            let data = serde_json::from_str(&raw).unwrap_or_default();
            (raw, data)
        }
    })
}
