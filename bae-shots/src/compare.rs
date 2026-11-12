//! Comparing a board's element list with a shot's. The hard checks live here:
//! what the shot lacks, what it adds, what it reads in another place, as
//! another kind, or in another state.

pub(crate) mod align;
pub(crate) mod normalize;

use crate::shots::Element;
use align::Step;
use normalize::{entries, Entry};

/// What differs between a matched board element and shot element.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Aspect {
    Value,
    Enabled,
    Selected,
}

/// A reason the run fails.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Finding {
    /// The board has it, the shot does not.
    Missing(Entry),
    /// The shot has it, the board does not.
    Extra(Entry),
    /// Both have it, in another place of the reading order.
    OutOfOrder {
        board: Entry,
        shot: Entry,
    },
    /// The same words at the same place, read as another kind.
    RoleDiffers {
        board: Entry,
        shot: Entry,
    },
    StateDiffers {
        board: Entry,
        shot: Entry,
        aspect: Aspect,
    },
}

/// The outcome of comparing two lists.
#[derive(Debug)]
pub(crate) struct Comparison {
    /// In the order the elements come in the reading order.
    pub(crate) findings: Vec<Finding>,
}

/// Compares the board's list with the shot's.
pub(crate) fn compare(board: &[Element], shot: &[Element]) -> Comparison {
    let board = entries(board);
    let shot = entries(shot);
    let board_keys: Vec<_> = board.iter().map(Entry::key).collect();
    let shot_keys: Vec<_> = shot.iter().map(Entry::key).collect();
    let steps = align::align(&board_keys, &shot_keys);

    // Findings with their step number, which orders them.
    let mut found: Vec<(usize, Finding)> = Vec::new();
    // Leftovers: (step, gap, entry). A gap is a run of steps between matches.
    let mut missing: Vec<(usize, usize, &Entry)> = Vec::new();
    let mut extra: Vec<(usize, usize, &Entry)> = Vec::new();
    let mut gap = 0;
    for (at, step) in steps.iter().enumerate() {
        match *step {
            Step::Match(b, s) => {
                gap += 1;
                for aspect in differences(&board[b], &shot[s]) {
                    found.push((
                        at,
                        Finding::StateDiffers {
                            board: board[b].clone(),
                            shot: shot[s].clone(),
                            aspect,
                        },
                    ));
                }
            }
            Step::Board(b) => missing.push((at, gap, &board[b])),
            Step::Shot(s) => extra.push((at, gap, &shot[s])),
        }
    }

    // The same kind and words left on both sides: one element out of order.
    let mut taken = vec![false; extra.len()];
    let mut missing_left = Vec::new();
    for &(at, gap, entry) in &missing {
        let same = extra.iter().enumerate().find(|(i, e)| !taken[*i] && e.2.key() == entry.key());
        match same {
            Some((i, other)) => {
                taken[i] = true;
                found.push((
                    at.min(other.0),
                    Finding::OutOfOrder { board: entry.clone(), shot: other.2.clone() },
                ));
            }
            None => missing_left.push((at, gap, entry)),
        }
    }
    // The same words in the same gap, another kind: one role difference.
    let mut extra_left: Vec<_> =
        extra.iter().enumerate().filter(|(i, _)| !taken[*i]).map(|(_, e)| *e).collect();
    for (at, gap, entry) in missing_left {
        let same = extra_left
            .iter()
            .position(|e| e.1 == gap && !entry.words.is_empty() && e.2.words == entry.words);
        match same {
            Some(i) => {
                let other = extra_left.remove(i);
                found.push((
                    at.min(other.0),
                    Finding::RoleDiffers { board: entry.clone(), shot: other.2.clone() },
                ));
            }
            None => found.push((at, Finding::Missing(entry.clone()))),
        }
    }
    for (at, _, entry) in extra_left {
        found.push((at, Finding::Extra(entry.clone())));
    }
    found.sort_by_key(|(at, _)| *at);
    Comparison { findings: found.into_iter().map(|(_, f)| f).collect() }
}

fn differences(board: &Entry, shot: &Entry) -> Vec<Aspect> {
    let mut differing = Vec::new();
    if board.value != shot.value {
        differing.push(Aspect::Value);
    }
    if board.enabled != shot.enabled {
        differing.push(Aspect::Enabled);
    }
    if board.selected != shot.selected {
        differing.push(Aspect::Selected);
    }
    differing
}

/// "the button “Set up”", or "the unlabelled button".
fn the(entry: &Entry) -> String {
    if entry.words.is_empty() {
        format!("the unlabelled {}", entry.kind.word())
    } else {
        format!("the {} “{}”", entry.kind.word(), entry.words)
    }
}

fn article(word: &str) -> &'static str {
    if word.starts_with(['a', 'e', 'i', 'o', 'u']) {
        "an"
    } else {
        "a"
    }
}

fn quoted(value: &str) -> String {
    if value.is_empty() {
        "no value".to_string()
    } else {
        format!("“{value}”")
    }
}

impl Finding {
    /// The finding as a sentence for the report.
    pub(crate) fn sentence(&self) -> String {
        match self {
            Self::Missing(entry) => format!("Shot is missing {}", the(entry)),
            Self::Extra(entry) => {
                format!("Shot has {}, which the board does not", the(entry))
            }
            Self::OutOfOrder { board, .. } => {
                let mut sentence = the(board);
                sentence.replace_range(..1, "T");
                format!("{sentence} is in another place in the shot's reading order")
            }
            Self::RoleDiffers { board, shot } => format!(
                "“{}” is {} {} on the board and {} {} in the shot",
                board.words,
                article(board.kind.word()),
                board.kind.word(),
                article(shot.kind.word()),
                shot.kind.word()
            ),
            Self::StateDiffers { board, shot, aspect } => {
                let mut subject = the(board);
                subject.replace_range(..1, "T");
                match aspect {
                    Aspect::Value => format!(
                        "{subject} has the value {} on the board and {} in the shot",
                        quoted(&board.value),
                        quoted(&shot.value)
                    ),
                    Aspect::Enabled => format!(
                        "{subject} is {} on the board and {} in the shot",
                        if board.enabled { "enabled" } else { "disabled" },
                        if shot.enabled { "enabled" } else { "disabled" }
                    ),
                    Aspect::Selected => format!(
                        "{subject} is {} on the board and {} in the shot",
                        if board.selected { "selected" } else { "not selected" },
                        if shot.selected { "selected" } else { "not selected" }
                    ),
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "compare_tests.rs"]
mod tests;
