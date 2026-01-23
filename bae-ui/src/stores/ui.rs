//! General UI state store (sidebar, search)

use dioxus::prelude::*;

/// State for the queue sidebar
#[derive(Clone, Debug, Default, PartialEq, Store)]
pub struct SidebarState {
    pub is_open: bool,
}

/// State for library search
#[derive(Clone, Debug, Default, PartialEq, Store)]
pub struct SearchState {
    pub query: String,
}

/// Combined UI state
#[derive(Clone, Debug, Default, PartialEq, Store)]
pub struct UiState {
    /// Queue sidebar state
    pub sidebar: SidebarState,
    /// Library search state
    pub search: SearchState,
}
