//! Saving and filling logins on web pages.
//!
//! A script in each web page notices a login form being sent and reports the username
//! and password, and the browser offers to save them. Filling only happens when the
//! person asks, from the toolbar, never on the page's own initiative.

use serde::Deserialize;

use crate::site_origin;

const MAX_USERNAME: usize = 256;
const MAX_PASSWORD: usize = 1024;

/// The site a login belongs to. Only HTTPS pages, and plain HTTP to this computer for
/// developing sites, can save logins, so a password never travels unencrypted.
pub fn login_origin(url: &str) -> Option<String> {
    let origin = site_origin(url)?;
    let host = origin
        .split_once("://")
        .map(|(_, host)| host.rsplit_once(':').map_or(host, |(host, _)| host))?;
    let is_local = host == "localhost" || host == "127.0.0.1" || host.ends_with(".localhost");
    (origin.starts_with("https://") || is_local).then_some(origin)
}

/// A login a page reported being sent.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SubmittedLogin {
    pub username: String,
    pub password: String,
}

impl SubmittedLogin {
    /// Reads a page's report. Pages control it, so anything odd is refused.
    pub fn parse(json: &str) -> Option<Self> {
        let login: Self = serde_json::from_str(json).ok()?;
        let clean = |text: &str| !text.chars().any(char::is_control);
        let fits = !login.password.is_empty()
            && login.password.chars().count() <= MAX_PASSWORD
            && login.username.chars().count() <= MAX_USERNAME;
        (fits && clean(&login.username) && clean(&login.password)).then_some(login)
    }
}

/// Watches a page for logins being sent. It's a function expression, called with the
/// function to report to, so the page's own scripts never see that function.
pub const LOGIN_WATCHER: &str = r#"(function (report) {
  const textTypes = ["text", "email", "tel", ""];
  function capture(scope) {
    const fields = [...(scope || document).querySelectorAll("input")];
    const password = fields.find((field) => field.type === "password" && field.value);
    if (!password) {
      return;
    }
    const before = fields.slice(0, fields.indexOf(password)).reverse();
    const user = before.find((field) => textTypes.includes(field.type) && field.value);
    report(JSON.stringify({ username: user ? user.value : "", password: password.value }));
  }
  document.addEventListener("submit", (event) => capture(event.target), true);
  document.addEventListener("keydown", (event) => {
    const field = event.target;
    if (event.key === "Enter" && field instanceof HTMLInputElement && field.type === "password") {
      capture(field.form);
    }
  }, true);
  document.addEventListener("click", (event) => {
    const button = event.target instanceof Element && event.target.closest("button, input[type=submit]");
    if (button) {
      capture(button.form);
    }
  }, true);
})"#;

/// A script that fills a login into the page's first password field and the text
/// field before it. The values are JSON-encoded, so they can't break out of the script.
pub fn fill_script(username: &str, password: &str) -> String {
    let username = serde_json::to_string(username).expect("Failed to encode a username");
    let password = serde_json::to_string(password).expect("Failed to encode a password");
    format!(
        r#"(() => {{
  const fields = [...document.querySelectorAll("input")];
  const password = fields.find((field) => field.type === "password");
  if (!password) {{
    return;
  }}
  const textTypes = ["text", "email", "tel", ""];
  const user = fields.slice(0, fields.indexOf(password)).reverse().find((field) => textTypes.includes(field.type));
  const set = (field, value) => {{
    field.focus();
    field.value = value;
    field.dispatchEvent(new Event("input", {{ bubbles: true }}));
    field.dispatchEvent(new Event("change", {{ bubbles: true }}));
  }};
  if (user) {{
    set(user, {username});
  }}
  set(password, {password});
}})();"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_secure_or_local_pages_save_logins() {
        assert_eq!(
            login_origin("https://Bank.example/login?next=/").as_deref(),
            Some("https://bank.example")
        );
        assert_eq!(
            login_origin("http://localhost:3000/login").as_deref(),
            Some("http://localhost:3000")
        );
        assert_eq!(
            login_origin("http://app.localhost/").as_deref(),
            Some("http://app.localhost")
        );
        assert_eq!(login_origin("http://bank.example/login"), None);
        assert_eq!(login_origin("http://localhost.evil.example/"), None);
        assert_eq!(login_origin("data:text/html,x"), None);
    }

    #[test]
    fn reads_reported_logins_with_limits() {
        let login = SubmittedLogin::parse(r#"{"username":"ann","password":"secret"}"#).unwrap();
        assert_eq!(login.username, "ann");
        assert!(SubmittedLogin::parse(r#"{"username":"ann","password":""}"#).is_none());
        assert!(SubmittedLogin::parse(r#"{"username":"a\nb","password":"x"}"#).is_none());
        let long = format!(r#"{{"username":"ann","password":"{}"}}"#, "x".repeat(2000));
        assert!(SubmittedLogin::parse(&long).is_none());
        assert!(SubmittedLogin::parse("not json").is_none());
    }

    #[test]
    fn filled_values_cant_break_out_of_the_script() {
        let script = fill_script("ann\"); alert(1); (\"", "p\"ss</script>");
        assert!(script.contains(r#"set(user, "ann\"); alert(1); (\"");"#));
        assert!(script.contains(r#"set(password, "p\"ss</script>");"#));
    }
}
