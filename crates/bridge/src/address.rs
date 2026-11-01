const ALLOWED_SCHEMES: [&str; 5] = ["http://", "https://", "file://", "about:", "chrome://"];

/// Turns address bar text into a URL: a URL as typed, a host name, or a search.
/// Other schemes, such as `javascript:` and `data:`, become searches so that pasted
/// text can't run in the current page.
pub fn resolve_input(input: &str, search_prefix: &str) -> Option<String> {
    let input = input.trim();
    if input.is_empty() {
        return None;
    }

    let lower = input.to_ascii_lowercase();
    if ALLOWED_SCHEMES
        .iter()
        .any(|scheme| lower.starts_with(scheme))
    {
        return Some(input.to_owned());
    }
    if input.contains(char::is_whitespace) {
        return Some(search_url(search_prefix, input));
    }

    let host_and_port = input.split(['/', '?', '#']).next().unwrap_or_default();
    let (host, port) = match host_and_port.rsplit_once(':') {
        Some((host, port)) => (host, Some(port)),
        None => (host_and_port, None),
    };
    if port.is_some_and(|port| port.is_empty() || !port.bytes().all(|b| b.is_ascii_digit())) {
        return Some(search_url(search_prefix, input));
    }

    let lower_host = host.to_ascii_lowercase();
    if lower_host == "localhost" || is_ipv4(host) {
        Some(format!("http://{input}"))
    } else if is_domain(&lower_host) {
        Some(format!("https://{input}"))
    } else {
        Some(search_url(search_prefix, input))
    }
}

fn is_ipv4(host: &str) -> bool {
    let parts: Vec<&str> = host.split('.').collect();
    parts.len() == 4 && parts.iter().all(|part| part.parse::<u8>().is_ok())
}

fn is_domain(host: &str) -> bool {
    let labels: Vec<&str> = host.split('.').collect();
    let valid_label = |label: &&str| {
        !label.is_empty()
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label.chars().all(|c| c.is_alphanumeric() || c == '-')
    };
    let top_level = labels.last().copied().unwrap_or_default();
    labels.len() >= 2
        && labels.iter().all(valid_label)
        && top_level.chars().count() >= 2
        && !top_level.chars().all(|c| c.is_ascii_digit())
}

fn search_url(search_prefix: &str, query: &str) -> String {
    let mut url = String::from(search_prefix);
    for byte in query.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                url.push(byte as char)
            }
            b' ' => url.push('+'),
            _ => url.push_str(&format!("%{byte:02X}")),
        }
    }
    url
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEARCH_URL: &str = "https://duckduckgo.com/?q=";

    fn resolved(input: &str) -> String {
        resolve_input(input, SEARCH_URL).unwrap()
    }

    #[test]
    fn empty_input_does_nothing() {
        assert_eq!(resolve_input("   ", SEARCH_URL), None);
    }

    #[test]
    fn keeps_full_urls() {
        assert_eq!(resolved(" https://a.com/x "), "https://a.com/x");
        assert_eq!(resolved("HTTP://a.com"), "HTTP://a.com");
        assert_eq!(resolved("about:blank"), "about:blank");
    }

    #[test]
    fn adds_https_to_domains() {
        assert_eq!(resolved("example.com"), "https://example.com");
        assert_eq!(
            resolved("sub.example.co.uk/a?b=1"),
            "https://sub.example.co.uk/a?b=1"
        );
        assert_eq!(resolved("example.com:8443/x"), "https://example.com:8443/x");
    }

    #[test]
    fn uses_http_for_local_addresses() {
        assert_eq!(resolved("localhost:3000"), "http://localhost:3000");
        assert_eq!(resolved("127.0.0.1:8080/a"), "http://127.0.0.1:8080/a");
    }

    #[test]
    fn searches_for_everything_else() {
        assert_eq!(resolved("rust"), "https://duckduckgo.com/?q=rust");
        assert_eq!(
            resolved("what is rust"),
            "https://duckduckgo.com/?q=what+is+rust"
        );
        assert_eq!(
            resolved("example.com is good"),
            "https://duckduckgo.com/?q=example.com+is+good"
        );
        assert_eq!(resolved("c++"), "https://duckduckgo.com/?q=c%2B%2B");
        assert_eq!(resolved("1.5"), "https://duckduckgo.com/?q=1.5");
    }

    #[test]
    fn uses_the_chosen_search_engine() {
        assert_eq!(
            resolve_input("rust", "https://s.example/?q=").unwrap(),
            "https://s.example/?q=rust"
        );
    }

    #[test]
    fn never_runs_script_urls() {
        assert!(resolved("javascript:alert(1)").starts_with(SEARCH_URL));
        assert!(resolved("JavaScript:alert(1)").starts_with(SEARCH_URL));
        assert!(resolved("data:text/html,hi").starts_with(SEARCH_URL));
    }
}
