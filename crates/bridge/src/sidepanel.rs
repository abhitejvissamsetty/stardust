//! What the side panel keeps: a page of notes, a to-do list and saved snippets. They're
//! shared by every window, and saved with the browser's other settings.

use serde::{Deserialize, Serialize};

pub const MAX_NOTES_CHARS: usize = 100_000;
pub const MAX_TODOS: usize = 200;
pub const MAX_TODO_CHARS: usize = 300;
pub const MAX_SNIPPETS: usize = 100;
pub const MAX_SNIPPET_TITLE_CHARS: usize = 80;
pub const MAX_SNIPPET_CHARS: usize = 5_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Todo {
    pub id: u32,
    pub text: String,
    pub done: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snippet {
    pub id: u32,
    pub title: String,
    pub text: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SidePanelData {
    pub notes: String,
    pub todos: Vec<Todo>,
    pub snippets: Vec<Snippet>,
    /// Ids are never reused within what's saved.
    #[serde(default)]
    next_id: u32,
}

fn clip(text: &str, limit: usize) -> String {
    text.chars().take(limit).collect()
}

/// A line of text with no control characters, such as a to-do or a title.
fn clean_line(text: &str, limit: usize) -> Option<String> {
    let line: String = text
        .chars()
        .map(|char| if char.is_control() { ' ' } else { char })
        .collect();
    let line = line.trim();
    (!line.is_empty()).then(|| clip(line, limit))
}

impl SidePanelData {
    /// Reads saved data. Damaged data gives an empty panel, and anything over a limit is
    /// cut back, since the file could have been edited.
    pub fn parse(json: &str) -> Self {
        let mut data: Self = serde_json::from_str(json).unwrap_or_default();
        data.notes = clip(&data.notes, MAX_NOTES_CHARS);
        data.todos.truncate(MAX_TODOS);
        data.todos
            .retain_mut(|todo| match clean_line(&todo.text, MAX_TODO_CHARS) {
                Some(text) => {
                    todo.text = text;
                    true
                }
                None => false,
            });
        data.snippets.truncate(MAX_SNIPPETS);
        data.snippets.retain_mut(|snippet| {
            let text = clip(&snippet.text, MAX_SNIPPET_CHARS);
            let title = clean_line(&snippet.title, MAX_SNIPPET_TITLE_CHARS)
                .or_else(|| clean_line(&text, MAX_SNIPPET_TITLE_CHARS));
            match title {
                Some(title) if !text.trim().is_empty() => {
                    snippet.title = title;
                    snippet.text = text;
                    true
                }
                _ => false,
            }
        });
        let highest = data
            .todos
            .iter()
            .map(|todo| todo.id)
            .chain(data.snippets.iter().map(|snippet| snippet.id))
            .max()
            .unwrap_or(0);
        data.next_id = data.next_id.max(highest);
        data
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("Failed to serialize the side panel")
    }

    fn take_id(&mut self) -> u32 {
        self.next_id += 1;
        self.next_id
    }

    pub fn set_notes(&mut self, notes: &str) {
        self.notes = clip(notes, MAX_NOTES_CHARS);
    }

    /// Adds a to-do at the top. False if it's empty or the list is full.
    pub fn add_todo(&mut self, text: &str) -> bool {
        let Some(text) = clean_line(text, MAX_TODO_CHARS) else {
            return false;
        };
        if self.todos.len() >= MAX_TODOS {
            return false;
        }
        let id = self.take_id();
        self.todos.insert(
            0,
            Todo {
                id,
                text,
                done: false,
            },
        );
        true
    }

    pub fn toggle_todo(&mut self, id: u32) -> bool {
        match self.todos.iter_mut().find(|todo| todo.id == id) {
            Some(todo) => {
                todo.done = !todo.done;
                true
            }
            None => false,
        }
    }

    pub fn remove_todo(&mut self, id: u32) -> bool {
        let before = self.todos.len();
        self.todos.retain(|todo| todo.id != id);
        self.todos.len() != before
    }

    pub fn clear_done(&mut self) -> bool {
        let before = self.todos.len();
        self.todos.retain(|todo| !todo.done);
        self.todos.len() != before
    }

    /// Adds a snippet at the top. A missing title is made from the text.
    pub fn add_snippet(&mut self, title: &str, text: &str) -> bool {
        let text = clip(text, MAX_SNIPPET_CHARS);
        if text.trim().is_empty() || self.snippets.len() >= MAX_SNIPPETS {
            return false;
        }
        let Some(title) = clean_line(title, MAX_SNIPPET_TITLE_CHARS)
            .or_else(|| clean_line(&text, MAX_SNIPPET_TITLE_CHARS))
        else {
            return false;
        };
        let id = self.take_id();
        self.snippets.insert(0, Snippet { id, title, text });
        true
    }

    pub fn remove_snippet(&mut self, id: u32) -> bool {
        let before = self.snippets.len();
        self.snippets.retain(|snippet| snippet.id != id);
        self.snippets.len() != before
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adds_toggles_and_removes_todos() {
        let mut data = SidePanelData::default();
        assert!(data.add_todo("  Film intro  "));
        assert!(data.add_todo("Edit thumbnail"));
        assert_eq!(data.todos[0].text, "Edit thumbnail");
        assert_eq!(data.todos[1].text, "Film intro");
        let id = data.todos[1].id;
        assert!(data.toggle_todo(id) && data.todos[1].done);
        assert!(data.clear_done());
        assert_eq!(data.todos.len(), 1);
        assert!(!data.toggle_todo(id) && !data.remove_todo(id));
        let remaining = data.todos[0].id;
        assert!(data.remove_todo(remaining) && data.todos.is_empty());
    }

    #[test]
    fn refuses_empty_and_oversize_items() {
        let mut data = SidePanelData::default();
        assert!(!data.add_todo("   ") && !data.add_todo("\u{7}\u{7}"));
        assert!(data.add_todo(&"x".repeat(MAX_TODO_CHARS + 50)));
        assert_eq!(data.todos[0].text.chars().count(), MAX_TODO_CHARS);
        assert!(!data.add_snippet("title", "   "));
        for number in 0..MAX_TODOS {
            data.add_todo(&format!("item {number}"));
        }
        assert_eq!(data.todos.len(), MAX_TODOS);
        assert!(!data.add_todo("one too many"));
    }

    #[test]
    fn control_characters_become_spaces() {
        let mut data = SidePanelData::default();
        data.add_todo("a\nb\tc");
        assert_eq!(data.todos[0].text, "a b c");
    }

    #[test]
    fn snippets_get_a_title_from_their_text() {
        let mut data = SidePanelData::default();
        assert!(data.add_snippet("", "Thanks for watching! Subscribe for more."));
        assert_eq!(
            data.snippets[0].title,
            "Thanks for watching! Subscribe for more."
        );
        assert!(data.add_snippet("Sign-off", "See you next week"));
        assert_eq!(data.snippets[0].title, "Sign-off");
        let id = data.snippets[0].id;
        assert!(data.remove_snippet(id) && !data.remove_snippet(id));
    }

    #[test]
    fn notes_are_cut_at_the_limit() {
        let mut data = SidePanelData::default();
        data.set_notes(&"n".repeat(MAX_NOTES_CHARS + 10));
        assert_eq!(data.notes.chars().count(), MAX_NOTES_CHARS);
        data.set_notes("line one\nline two");
        assert_eq!(data.notes, "line one\nline two");
    }

    #[test]
    fn ids_are_not_reused_after_saving() {
        let mut data = SidePanelData::default();
        data.add_todo("one");
        data.add_snippet("s", "text");
        let reloaded = SidePanelData::parse(&data.to_json());
        assert_eq!(reloaded, data);
        let mut reloaded = reloaded;
        reloaded.add_todo("two");
        let ids: Vec<u32> = reloaded.todos.iter().map(|todo| todo.id).collect();
        assert_eq!(ids, [3, 1]);
    }

    #[test]
    fn saved_data_is_checked_when_read() {
        let json = r#"{"notes":"hi","todos":[
            {"id":4,"text":"","done":false},
            {"id":5,"text":"ok\u0007","done":true}
        ],"snippets":[
            {"id":9,"title":"","text":"  "},
            {"id":10,"title":"t","text":"body"}
        ],"next_id":0}"#;
        let data = SidePanelData::parse(json);
        assert_eq!(data.todos.len(), 1);
        assert_eq!(data.todos[0].text, "ok");
        assert_eq!(data.snippets.len(), 1);
        let mut data = data;
        data.add_todo("new");
        assert_eq!(data.todos[0].id, 11);
        assert_eq!(SidePanelData::parse("not json"), SidePanelData::default());
        assert_eq!(SidePanelData::parse(""), SidePanelData::default());
    }
}
