//! Shots declared with no board to compare against. A screen shot's board is
//! `design/mocks/<name>.dc.html`; a component shot's board is the named row of
//! `design/system/components/<Component>/preview.html`, or the one state
//! `default` when the preview names no row.

use crate::declared::Declared;
use crate::names::{state_id, Name};
use crate::record::{Record, FILE};
use regex::Regex;
use std::path::Path;

const BOARDS: &str = "design/mocks";
const COMPONENTS: &str = "design/system/components";

/// The ids of the named rows of a preview page: each `N('…', …)` or
/// `W('…', …)` call.
fn states(html: &str) -> Vec<String> {
    let row = Regex::new(r"\b[NW]\('((?:[^'\\]|\\.)*)'").unwrap();
    row.captures_iter(html).map(|c| state_id(&c[1].replace("\\'", "'"))).collect()
}

/// A shot or a record entry with no board.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Problem {
    pub(crate) name: String,
    /// True for a shot a harness declares, false for an entry of the record.
    pub(crate) declared: bool,
    pub(crate) sentence: String,
}

/// One problem for each declared shot and each entry of the record that has
/// no board.
pub(crate) fn problems(root: &Path, declared: &[Declared], record: &Record) -> Vec<Problem> {
    let shots = declared.iter().filter_map(|d| {
        no_board(root, &d.name).map(|why| Problem {
            name: d.name.to_string(),
            declared: true,
            sentence: format!("shot '{}' has no board: {why}", d.name),
        })
    });
    let entries = record.names().filter_map(|text| {
        let why = match Name::parse(text) {
            Ok(name) => no_board(root, &name)?,
            Err(e) => e.to_string(),
        };
        Some(Problem {
            name: text.clone(),
            declared: false,
            sentence: format!("{FILE} has an entry '{text}' with no board: {why}"),
        })
    });
    shots.chain(entries).collect()
}

/// Why `name` has no board, when it has none.
fn no_board(root: &Path, name: &Name) -> Option<String> {
    match name {
        Name::Screen(_) => {
            let board = Path::new(BOARDS).join(format!("{name}.dc.html"));
            (!root.join(&board).is_file()).then(|| format!("{} is missing", board.display()))
        }
        Name::Component(c) => {
            let preview = Path::new(COMPONENTS).join(c.component()).join("preview.html");
            let Ok(html) = std::fs::read_to_string(root.join(&preview)) else {
                return Some(format!("{} is missing", preview.display()));
            };
            let mut states = states(&html);
            if states.is_empty() {
                states.push("default".to_string());
            }
            (!states.iter().any(|s| s == c.state()))
                .then(|| format!("{} has the states {}", c.component(), states.join(", ")))
        }
    }
}

#[cfg(test)]
#[path = "lint_tests.rs"]
mod tests;
