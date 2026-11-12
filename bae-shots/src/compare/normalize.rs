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

/// The board's entries in the shot's terms, where the shot's tree merges what
/// the board keeps apart: a field named with the supporting text under it
/// (Compose puts both in the field's one name). A field followed by a run of
/// texts whose words, joined in order, are a field of the shot is that one
/// field; a run that does not join to something the shot holds is left as it
/// is, and the comparison reports it.
pub(crate) fn fold_merged(board: Vec<Entry>, shot: &[Entry]) -> Vec<Entry> {
    // The shot's keys not yet taken by a fold.
    let mut available: Vec<_> = shot.iter().map(Entry::key).collect();
    let mut folded = Vec::with_capacity(board.len());
    let mut at = 0;
    while at < board.len() {
        let head = &board[at];
        let run = if head.kind == Role::Field {
            board[at + 1..].iter().take_while(|e| e.kind == Role::Text).count()
        } else {
            0
        };
        let merged = (1..=run).rev().find_map(|tail| {
            let mut parts = vec![head.words.as_str()];
            parts.extend(board[at + 1..=at + tail].iter().map(|e| e.words.as_str()));
            let key = (head.kind, parts.join(" "));
            available.iter().position(|k| *k == key).map(|place| (tail, place, key))
        });
        match merged {
            Some((tail, place, key)) => {
                available.swap_remove(place);
                folded.push(Entry { words: key.1, ..head.clone() });
                at += tail + 1;
            }
            None => {
                folded.push(head.clone());
                at += 1;
            }
        }
    }
    folded
}

#[cfg(test)]
#[path = "normalize_tests.rs"]
pub(crate) mod tests;
