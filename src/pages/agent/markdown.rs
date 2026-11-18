//! Markdown → HTML for agent answers. Raw HTML from the model is escaped, unsafe link schemes removed.

use dioxus::prelude::*;
use pulldown_cmark::{CowStr, Event, Options, Parser, Tag, html};

/// Typography for rendered markdown (theme variables only).
const MD_CLASS: &str = "min-w-0 text-sm leading-relaxed break-words \
[&>*:first-child]:mt-0 [&>*:last-child]:mb-0 [&_p]:my-2 \
[&_ul]:my-2 [&_ul]:list-disc [&_ul]:pl-5 [&_ol]:my-2 [&_ol]:list-decimal [&_ol]:pl-5 [&_li]:my-0.5 \
[&_h1]:mt-4 [&_h1]:mb-2 [&_h1]:text-lg [&_h1]:font-semibold [&_h2]:mt-4 [&_h2]:mb-2 [&_h2]:text-base [&_h2]:font-semibold \
[&_h3]:mt-3 [&_h3]:mb-1 [&_h3]:font-semibold [&_strong]:font-semibold \
[&_code]:rounded [&_code]:bg-muted [&_code]:px-1 [&_code]:py-0.5 [&_code]:font-mono [&_code]:text-[0.85em] \
[&_pre]:my-2 [&_pre]:overflow-x-auto [&_pre]:rounded-md [&_pre]:border [&_pre]:bg-muted/50 [&_pre]:p-3 [&_pre]:text-xs \
[&_pre_code]:bg-transparent [&_pre_code]:p-0 \
[&_a]:text-info [&_a]:underline [&_a]:underline-offset-2 \
[&_table]:my-2 [&_table]:block [&_table]:max-w-full [&_table]:overflow-x-auto [&_table]:text-xs \
[&_th]:border [&_th]:px-2 [&_th]:py-1 [&_th]:text-left [&_th]:font-medium [&_th]:bg-muted/50 [&_td]:border [&_td]:px-2 [&_td]:py-1 \
[&_blockquote]:my-2 [&_blockquote]:border-l-2 [&_blockquote]:pl-3 [&_blockquote]:text-muted-foreground [&_hr]:my-3";

fn safe_url(url: CowStr<'_>) -> CowStr<'_> {
    let lower = url.trim().to_ascii_lowercase();
    let scheme_end = lower.find(':');
    let first_sep = lower.find(['/', '?', '#']);
    let has_scheme = match (scheme_end, first_sep) {
        (Some(c), Some(s)) => c < s,
        (Some(_), None) => true,
        _ => false,
    };
    if !has_scheme
        || lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("mailto:")
    {
        url
    } else {
        CowStr::Borrowed("#")
    }
}

pub fn to_html(md: &str) -> String {
    let opts = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let parser = Parser::new_ext(md, opts).map(|ev| match ev {
        Event::Html(s) | Event::InlineHtml(s) => Event::Text(s),
        Event::Start(Tag::Link {
            link_type,
            dest_url,
            title,
            id,
        }) => Event::Start(Tag::Link {
            link_type,
            dest_url: safe_url(dest_url),
            title,
            id,
        }),
        Event::Start(Tag::Image {
            link_type,
            dest_url,
            title,
            id,
        }) => Event::Start(Tag::Image {
            link_type,
            dest_url: safe_url(dest_url),
            title,
            id,
        }),
        e => e,
    });
    let mut out = String::new();
    html::push_html(&mut out, parser);
    // Raw HTML is escaped above, so every `<a href=` here comes from a markdown link.
    out.replace(
        "<a href=",
        "<a target=\"_blank\" rel=\"noopener noreferrer\" href=",
    )
}

#[component]
pub fn Markdown(text: String) -> Element {
    let html = use_memo(use_reactive!(|text| to_html(&text)));
    rsx! { div { class: MD_CLASS, dangerous_inner_html: "{html}" } }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_raw_html_and_bad_links() {
        let h = to_html(
            "Hallo <script>alert(1)</script> **fett**\n\n<div onclick=x>y</div>\n\n[a](javascript:alert(1)) [b](https://x.de)",
        );
        assert!(!h.contains("<script>"), "{h}");
        assert!(!h.contains("<div"), "{h}");
        assert!(h.contains("&lt;script&gt;"));
        assert!(h.contains("<strong>fett</strong>"));
        assert!(h.contains("href=\"#\""));
        assert!(h.contains("target=\"_blank\" rel=\"noopener noreferrer\" href=\"https://x.de\""));
    }
}
