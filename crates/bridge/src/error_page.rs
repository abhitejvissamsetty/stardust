/// A page explaining a failed load. The URL and error come from the network or the
/// site, so they are escaped.
pub fn error_page(url: &str, error: &str) -> String {
    let url = escape_html(url);
    let error = escape_html(error);
    format!(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><title>Can't open this page</title>
<style>
:root{{color-scheme:light dark;--text:#1c1d21;--muted:#62656e;--bg:#fff}}
@media (prefers-color-scheme:dark){{:root{{--text:#e8e9ec;--muted:#a0a3ad;--bg:#16171a}}}}
body{{background:var(--bg);color:var(--text);font:15px/1.5 system-ui,sans-serif;margin:0;display:grid;place-items:center;min-height:100vh}}
main{{max-width:520px;padding:24px;overflow-wrap:anywhere}}
h1{{font-size:22px;font-weight:600;margin:0 0 8px}}
p{{color:var(--muted);margin:0 0 8px}}
code{{font-size:13px}}
</style></head><body><main><h1>Can't open this page</h1><p>{url}</p><p><code>{error}</code></p></main></body></html>"#
    )
}

fn escape_html(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(c),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_the_url_and_error() {
        let page = error_page("https://a.com/<script>", "ERR\"<b>");
        assert!(!page.contains("<script>"));
        assert!(!page.contains("<b>"));
        assert!(page.contains("&lt;script&gt;"));
    }
}
