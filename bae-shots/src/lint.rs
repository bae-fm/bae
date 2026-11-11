//! Shots declared with no board to compare against. A screen shot's board is
//! `design/mocks/<name>.dc.html`; a component shot's board is the named row of
//! `design/system/components/<Component>/preview.html`, or the one state
//! `default` when the preview names no row.

use crate::declared::Declared;
use crate::names::{state_id, Name};
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

/// One sentence for each declared shot that has no board.
pub(crate) fn problems(root: &Path, declared: &[Declared]) -> Vec<String> {
    declared.iter().filter_map(|d| problem(root, d)).collect()
}

fn problem(root: &Path, declared: &Declared) -> Option<String> {
    let name = &declared.name;
    match name {
        Name::Screen(_) => {
            let board = Path::new(BOARDS).join(format!("{name}.dc.html"));
            (!root.join(&board).is_file())
                .then(|| format!("shot '{name}' has no board: {} is missing", board.display()))
        }
        Name::Component(c) => {
            let preview = Path::new(COMPONENTS).join(c.component()).join("preview.html");
            let Ok(html) = std::fs::read_to_string(root.join(&preview)) else {
                return Some(format!(
                    "shot '{name}' has no board: {} is missing",
                    preview.display()
                ));
            };
            let mut states = states(&html);
            if states.is_empty() {
                states.push("default".to_string());
            }
            (!states.iter().any(|s| s == c.state())).then(|| {
                format!(
                    "shot '{name}' has no board: {} has the states {}",
                    c.component(),
                    states.join(", ")
                )
            })
        }
    }
}

#[cfg(test)]
#[path = "lint_tests.rs"]
mod tests;
