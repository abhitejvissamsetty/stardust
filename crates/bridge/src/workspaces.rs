//! Workspaces: a named set of tabs with its own logins and cookies, for keeping one
//! channel, brand or project apart from the others.

use serde::{Deserialize, Serialize};

/// The workspace every profile starts with. It uses the browser's original profile and
/// has no entry in the list, so it can't be renamed or removed. Its id is empty.
pub const MAIN_WORKSPACE: &str = "";
pub const MAIN_NAME: &str = "Personal";
pub const MAIN_COLOR: &str = "blue";
pub const MAX_WORKSPACES: usize = 20;
pub const MAX_NAME_CHARS: usize = 40;

/// Colours are chosen from this list, never taken as free text, so a saved file can't
/// put anything else into the interface.
pub const COLORS: [&str; 8] = [
    "blue", "green", "red", "amber", "purple", "pink", "teal", "gray",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub color: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Workspaces {
    items: Vec<Workspace>,
    /// Numbers are never reused, so a removed workspace's data can't come back under a
    /// new one.
    #[serde(default)]
    next: u32,
}

/// An id made by this module: `w` and a number. Ids become folder names, so nothing
/// else is accepted.
pub fn is_valid_id(id: &str) -> bool {
    id.len() > 1
        && id.len() <= 10
        && id.starts_with('w')
        && id[1..].bytes().all(|byte| byte.is_ascii_digit())
}

pub fn clean_name(name: &str) -> Option<String> {
    let name = name.trim();
    let valid = !name.is_empty()
        && name.chars().count() <= MAX_NAME_CHARS
        && !name.chars().any(char::is_control);
    valid.then(|| name.to_owned())
}

pub fn clean_color(color: &str) -> &'static str {
    COLORS
        .iter()
        .find(|known| **known == color)
        .copied()
        .unwrap_or("blue")
}

impl Workspaces {
    /// Reads the saved list. Damaged data gives an empty list, and entries with a bad
    /// id or name are dropped.
    pub fn parse(json: &str) -> Self {
        let mut saved: Self = serde_json::from_str(json).unwrap_or_default();
        let mut seen = std::collections::HashSet::new();
        saved.items.retain(|item| {
            is_valid_id(&item.id)
                && clean_name(&item.name).is_some()
                && seen.insert(item.id.clone())
        });
        saved.items.truncate(MAX_WORKSPACES);
        for item in &mut saved.items {
            item.name = clean_name(&item.name).unwrap_or_default();
            item.color = clean_color(&item.color).to_owned();
        }
        let highest = saved
            .items
            .iter()
            .filter_map(|item| item.id[1..].parse::<u32>().ok())
            .max()
            .unwrap_or(0);
        saved.next = saved.next.max(highest);
        saved
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("Failed to serialize workspaces")
    }

    pub fn items(&self) -> &[Workspace] {
        &self.items
    }

    /// True for the main workspace and for any saved one.
    pub fn exists(&self, id: &str) -> bool {
        id == MAIN_WORKSPACE || self.items.iter().any(|item| item.id == id)
    }

    /// Name and colour for any workspace, the main one included. Unknown ids show as main.
    pub fn display(&self, id: &str) -> (String, String) {
        self.items
            .iter()
            .find(|item| item.id == id)
            .map(|item| (item.name.clone(), item.color.clone()))
            .unwrap_or_else(|| (MAIN_NAME.to_owned(), MAIN_COLOR.to_owned()))
    }

    pub fn create(&mut self, name: &str, color: &str) -> Result<String, &'static str> {
        let name = clean_name(name).ok_or("Give the workspace a name of up to 40 characters.")?;
        if self.items.len() >= MAX_WORKSPACES {
            return Err("That's the most workspaces there can be.");
        }
        if self.name_taken(&name, None) {
            return Err("Another workspace already has that name.");
        }
        self.next += 1;
        let id = format!("w{}", self.next);
        self.items.push(Workspace {
            id: id.clone(),
            name,
            color: clean_color(color).to_owned(),
        });
        Ok(id)
    }

    pub fn rename(&mut self, id: &str, name: &str) -> Result<(), &'static str> {
        let name = clean_name(name).ok_or("Give the workspace a name of up to 40 characters.")?;
        if self.name_taken(&name, Some(id)) {
            return Err("Another workspace already has that name.");
        }
        let item = self
            .items
            .iter_mut()
            .find(|item| item.id == id)
            .ok_or("That workspace doesn't exist.")?;
        item.name = name;
        Ok(())
    }

    pub fn set_color(&mut self, id: &str, color: &str) -> bool {
        match self.items.iter_mut().find(|item| item.id == id) {
            Some(item) => {
                item.color = clean_color(color).to_owned();
                true
            }
            None => false,
        }
    }

    pub fn remove(&mut self, id: &str) -> bool {
        let before = self.items.len();
        self.items.retain(|item| item.id != id);
        self.items.len() != before
    }

    fn name_taken(&self, name: &str, except: Option<&str>) -> bool {
        let same = |other: &str| other.to_lowercase() == name.to_lowercase();
        same(MAIN_NAME)
            || self
                .items
                .iter()
                .any(|item| Some(item.id.as_str()) != except && same(&item.name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_and_numbers_workspaces() {
        let mut list = Workspaces::default();
        assert_eq!(list.create("Channel A", "green"), Ok("w1".to_owned()));
        assert_eq!(list.create("  Channel B ", "red"), Ok("w2".to_owned()));
        assert_eq!(list.items()[1].name, "Channel B");
        assert!(list.exists("w1") && list.exists(MAIN_WORKSPACE) && !list.exists("w3"));
    }

    #[test]
    fn ids_are_never_reused() {
        let mut list = Workspaces::default();
        list.create("One", "blue").unwrap();
        assert!(list.remove("w1"));
        assert_eq!(list.create("Two", "blue"), Ok("w2".to_owned()));
        let reloaded = Workspaces::parse(&list.to_json());
        assert_eq!(reloaded, list);
    }

    #[test]
    fn rejects_bad_and_duplicate_names() {
        let mut list = Workspaces::default();
        assert!(list.create("", "blue").is_err());
        assert!(list.create("   ", "blue").is_err());
        assert!(list.create(&"x".repeat(41), "blue").is_err());
        assert!(list.create("a\u{7}b", "blue").is_err());
        list.create("Studio", "blue").unwrap();
        assert!(list.create("studio", "blue").is_err());
        assert!(list.create("personal", "blue").is_err());
        let id = list.create("Other", "blue").unwrap();
        assert!(list.rename(&id, "STUDIO").is_err());
        assert!(list.rename(&id, "Other").is_ok());
        assert!(list.rename("w99", "Anything").is_err());
    }

    #[test]
    fn limits_how_many_there_can_be() {
        let mut list = Workspaces::default();
        for number in 0..MAX_WORKSPACES {
            list.create(&format!("Space {number}"), "blue").unwrap();
        }
        assert!(list.create("One too many", "blue").is_err());
    }

    #[test]
    fn unknown_colours_become_blue() {
        let mut list = Workspaces::default();
        list.create("A", "javascript:alert(1)").unwrap();
        assert_eq!(list.items()[0].color, "blue");
        assert!(list.set_color("w1", "pink") && list.items()[0].color == "pink");
        assert!(!list.set_color("w9", "pink"));
    }

    #[test]
    fn saved_data_is_checked_when_read() {
        let json = r#"{"items":[
            {"id":"w1","name":"Good","color":"teal"},
            {"id":"../etc","name":"Bad id","color":"blue"},
            {"id":"w2","name":"","color":"blue"},
            {"id":"w1","name":"Duplicate","color":"blue"},
            {"id":"w3","name":"Odd colour","color":"<script>"}
        ],"next":0}"#;
        let list = Workspaces::parse(json);
        let names: Vec<_> = list.items().iter().map(|item| item.name.as_str()).collect();
        assert_eq!(names, ["Good", "Odd colour"]);
        assert_eq!(list.items()[1].color, "blue");
        // The counter catches up with the highest saved id.
        let mut list = list;
        assert_eq!(list.create("New", "blue"), Ok("w4".to_owned()));
        assert!(Workspaces::parse("not json").items().is_empty());
        assert!(Workspaces::parse("").items().is_empty());
    }

    #[test]
    fn only_made_ids_are_valid() {
        assert!(is_valid_id("w1") && is_valid_id("w123"));
        for bad in ["", "w", "w1/", "../w1", "W1", "w1a", "w12345678901", " w1"] {
            assert!(!is_valid_id(bad), "{bad}");
        }
    }

    #[test]
    fn shows_the_main_workspace_for_unknown_ids() {
        let list = Workspaces::default();
        assert_eq!(list.display(MAIN_WORKSPACE).0, MAIN_NAME);
        assert_eq!(list.display("w9").0, MAIN_NAME);
    }
}
