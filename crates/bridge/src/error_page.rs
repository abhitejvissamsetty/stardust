use serde::Serialize;

/// A page explaining a failed load. The URL and error come from the network or the
/// site, so the page is built from text, never parsed as HTML.
#[derive(Debug, Serialize)]
pub struct ErrorPage {
    pub heading: &'static str,
    pub paragraphs: Vec<String>,
    pub error: String,
    pub style: &'static str,
}

const STYLE: &str = "\
:root{color-scheme:light dark;--text:#1c1d21;--muted:#62656e;--bg:#fff}
@media (prefers-color-scheme:dark){:root{--text:#e8e9ec;--muted:#a0a3ad;--bg:#16171a}}
body{background:var(--bg);color:var(--text);font:15px/1.5 system-ui,sans-serif;margin:0;display:grid;place-items:center;min-height:100vh}
main{max-width:520px;padding:24px;overflow-wrap:anywhere}
h1{font-size:22px;font-weight:600;margin:0 0 8px}
p{color:var(--muted);margin:0 0 8px}
code{font-size:13px}";

/// Chromium numbers certificate errors from -200 to -299.
pub fn is_certificate_error(code: i32) -> bool {
    (-299..=-200).contains(&code)
}

pub fn error_page(url: &str, error: &str, code: i32) -> ErrorPage {
    let mut paragraphs = Vec::new();
    // There's no way past a certificate error.
    let heading = if is_certificate_error(code) {
        paragraphs.push(
            "The site's security certificate isn't valid, so someone could read or change what you send it. Stardust stopped loading it."
                .to_owned(),
        );
        "This connection isn't secure"
    } else {
        "Can't open this page"
    };
    paragraphs.push(url.to_owned());
    ErrorPage {
        heading,
        paragraphs,
        error: error.to_owned(),
        style: STYLE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_the_url_and_error_as_text() {
        let page = error_page("https://a.com/<script>", "ERR\"<b>", -105);
        assert_eq!(page.heading, "Can't open this page");
        assert_eq!(page.paragraphs, ["https://a.com/<script>"]);
        assert_eq!(page.error, "ERR\"<b>");
    }

    #[test]
    fn says_when_a_connection_isnt_secure() {
        let page = error_page("https://a.com", "ERR_CERT_DATE_INVALID", -201);
        assert_eq!(page.heading, "This connection isn't secure");
        assert_eq!(page.paragraphs.len(), 2);
        assert!(!is_certificate_error(-105));
    }
}
