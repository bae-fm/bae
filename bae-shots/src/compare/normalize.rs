//! Bringing a board's element list and a shot's to the same terms before they
//! are aligned: words as `shots::words` makes them, the text inside a named
//! control folded into the control's name, and elements with no words (which
//! are structure, or pictures the regions check covers) left out.

use crate::shots::{words, Element, Role};

/// An element as the comparison sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Entry {
    /// The element's index in the list it came from.
    pub(crate) index: usize,
    pub(crate) kind: Role,
    pub(crate) words: String,
    /// A field's text, `on` or `off`, a pop-up's choice; empty when none.
    pub(crate) value: String,
    pub(crate) enabled: bool,
    pub(crate) selected: bool,
}

impl Entry {
    pub(crate) fn key(&self) -> (Role, String) {
        (self.kind, self.words.clone())
    }
}

/// The words an element is known by: its label, or for a field with none the
/// placeholder it shows.
fn name(element: &Element) -> String {
    let label = element.label.as_deref().map(words).unwrap_or_default();
    match (&element.placeholder, label.is_empty(), element.role) {
        (Some(placeholder), true, Role::Field) => words(placeholder),
        _ => label,
    }
}

/// The entries of `list`, in its order.
pub(crate) fn entries(list: &[Element]) -> Vec<Entry> {
    // Whether a named control holds the element, directly or further up.
    let mut inside_control = vec![false; list.len()];
    let mut entries = Vec::new();
    for (index, element) in list.iter().enumerate() {
        let words = name(element);
        if let Some(parent) = element.parent.filter(|p| *p < index) {
            let holder = &list[parent];
            inside_control[index] =
                inside_control[parent] || (holder.role.is_control() && !name(holder).is_empty());
        }
        let structure = match element.role {
            Role::Text | Role::Image | Role::Other => words.is_empty(),
            _ => false,
        };
        if structure || (element.role == Role::Text && inside_control[index]) {
            continue;
        }
        entries.push(Entry {
            index,
            kind: element.role,
            words,
            value: element.value.as_deref().map(crate::shots::words).unwrap_or_default(),
            enabled: element.enabled,
            selected: element.selected.unwrap_or(false),
        });
    }
    entries
}

#[cfg(test)]
#[path = "normalize_tests.rs"]
pub(crate) mod tests;
