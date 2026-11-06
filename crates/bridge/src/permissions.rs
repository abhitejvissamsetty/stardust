//! What each site may use: the camera, the microphone or the computer's location.
//! The person decides once per site, and the decision is saved.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Permission {
    Camera,
    Microphone,
    Location,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Decision {
    Allow,
    Block,
}

/// What to do with a request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    Allow,
    Block,
    /// Ask the person about these.
    Ask(Vec<Permission>),
}

/// Saved decisions by site, such as `https://meet.example`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SitePermissions {
    sites: BTreeMap<String, BTreeMap<Permission, Decision>>,
}

/// A page's site for permissions: its scheme, host and port. Only web pages have one.
pub fn site_origin(url: &str) -> Option<String> {
    let (scheme, rest) = url.split_once("://")?;
    let scheme = scheme.to_ascii_lowercase();
    if scheme != "https" && scheme != "http" {
        return None;
    }
    let host = rest.split(['/', '?', '#']).next()?.to_ascii_lowercase();
    // Addresses with a user name could make one site look like another.
    if host.is_empty() || host.contains('@') {
        return None;
    }
    Some(format!("{scheme}://{host}"))
}

/// Words for the prompt: "use your camera and microphone".
pub fn describe(permissions: &[Permission]) -> String {
    let wants_camera = permissions.contains(&Permission::Camera);
    let wants_microphone = permissions.contains(&Permission::Microphone);
    let media = match (wants_camera, wants_microphone) {
        (true, true) => Some("use your camera and microphone"),
        (true, false) => Some("use your camera"),
        (false, true) => Some("use your microphone"),
        (false, false) => None,
    };
    let location = permissions
        .contains(&Permission::Location)
        .then_some("know your location");
    match (media, location) {
        (Some(media), Some(location)) => format!("{media}, and {location}"),
        (Some(text), None) | (None, Some(text)) => text.to_owned(),
        (None, None) => String::new(),
    }
}

impl SitePermissions {
    /// Reads saved decisions. Damaged data or sites that aren't web pages are dropped.
    pub fn parse(json: &str) -> Self {
        let mut saved: Self = serde_json::from_str(json).unwrap_or_default();
        saved.sites.retain(|site, decisions| {
            site_origin(site).as_deref() == Some(site.as_str()) && !decisions.is_empty()
        });
        saved
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("Failed to save site permissions")
    }

    /// A blocked permission blocks the whole request. Otherwise, anything not yet
    /// decided is asked about.
    pub fn answer(&self, site: &str, requested: &[Permission]) -> Answer {
        if requested.is_empty() {
            return Answer::Block;
        }
        let decisions = self.sites.get(site);
        let decision = |permission: &Permission| {
            decisions.and_then(|decisions| decisions.get(permission).copied())
        };
        if requested
            .iter()
            .any(|permission| decision(permission) == Some(Decision::Block))
        {
            return Answer::Block;
        }
        let undecided: Vec<Permission> = requested
            .iter()
            .copied()
            .filter(|permission| decision(permission).is_none())
            .collect();
        if undecided.is_empty() {
            Answer::Allow
        } else {
            Answer::Ask(requested.to_vec())
        }
    }

    pub fn remember(&mut self, site: &str, permissions: &[Permission], decision: Decision) {
        if site_origin(site).as_deref() != Some(site) {
            return;
        }
        let decisions = self.sites.entry(site.to_owned()).or_default();
        for permission in permissions {
            decisions.insert(*permission, decision);
        }
    }

    pub fn forget(&mut self, site: &str) {
        self.sites.remove(site);
    }

    /// For the Settings page.
    pub fn sites(&self) -> &BTreeMap<String, BTreeMap<Permission, Decision>> {
        &self.sites
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SITE: &str = "https://meet.example";

    #[test]
    fn finds_a_pages_site() {
        assert_eq!(
            site_origin("https://Meet.Example/room?x=1").as_deref(),
            Some(SITE)
        );
        assert_eq!(
            site_origin("http://localhost:3000/").as_deref(),
            Some("http://localhost:3000")
        );
        assert_eq!(site_origin("file:///etc/passwd"), None);
        assert_eq!(site_origin("data:text/html,x"), None);
        assert_eq!(site_origin("https://user@evil.example/"), None);
    }

    #[test]
    fn asks_about_anything_undecided() {
        let mut saved = SitePermissions::default();
        assert_eq!(
            saved.answer(SITE, &[Permission::Camera]),
            Answer::Ask(vec![Permission::Camera])
        );
        saved.remember(SITE, &[Permission::Camera], Decision::Allow);
        assert_eq!(saved.answer(SITE, &[Permission::Camera]), Answer::Allow);
        assert_eq!(
            saved.answer(SITE, &[Permission::Camera, Permission::Microphone]),
            Answer::Ask(vec![Permission::Camera, Permission::Microphone])
        );
        assert_eq!(
            saved.answer("https://other.example", &[Permission::Camera]),
            Answer::Ask(vec![Permission::Camera])
        );
    }

    #[test]
    fn one_block_blocks_the_request() {
        let mut saved = SitePermissions::default();
        saved.remember(SITE, &[Permission::Camera], Decision::Allow);
        saved.remember(SITE, &[Permission::Microphone], Decision::Block);
        assert_eq!(
            saved.answer(SITE, &[Permission::Camera, Permission::Microphone]),
            Answer::Block
        );
        assert_eq!(saved.answer(SITE, &[]), Answer::Block);
    }

    #[test]
    fn decisions_survive_saving_and_can_be_forgotten() {
        let mut saved = SitePermissions::default();
        saved.remember(SITE, &[Permission::Location], Decision::Block);
        let mut read = SitePermissions::parse(&saved.to_json());
        assert_eq!(read, saved);
        read.forget(SITE);
        assert_eq!(
            read.answer(SITE, &[Permission::Location]),
            Answer::Ask(vec![Permission::Location])
        );
    }

    #[test]
    fn edited_data_cant_add_other_kinds_of_site() {
        let json = r#"{"sites":{"file:///":{"camera":"allow"},"https://a.example":{"camera":"allow"},"https://b.example/path":{"camera":"allow"}}}"#;
        let read = SitePermissions::parse(json);
        assert_eq!(
            read.sites().keys().collect::<Vec<_>>(),
            ["https://a.example"]
        );
        assert_eq!(
            SitePermissions::parse("garbage"),
            SitePermissions::default()
        );
        let mut saved = SitePermissions::default();
        saved.remember("file:///", &[Permission::Camera], Decision::Allow);
        assert!(saved.sites().is_empty());
    }

    #[test]
    fn describes_requests_in_words() {
        assert_eq!(
            describe(&[Permission::Camera, Permission::Microphone]),
            "use your camera and microphone"
        );
        assert_eq!(describe(&[Permission::Location]), "know your location");
        assert_eq!(
            describe(&[Permission::Microphone, Permission::Location]),
            "use your microphone, and know your location"
        );
    }
}
