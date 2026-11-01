//! Plain lists of saved items sent to the toolbar.

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SuggestionItem {
    pub url: String,
    pub title: String,
    pub is_bookmark: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FolderItems {
    pub name: String,
    pub items: Vec<SuggestionItem>,
}
