use serde::{Deserialize, Serialize};

/// One piece of a name pattern: a value the save fills in, or text the person
/// typed, kept as typed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NamePiece<Value> {
    Value(Value),
    Text(String),
}

/// The characters typed text may not hold: the ones a name can't have on
/// every volume a save writes to.
const FORBIDDEN: [char; 9] = ['\\', ':', '?', '*', '"', '<', '>', '|', '/'];

/// Why a pattern can't make a name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NamePatternError {
    /// The pattern has no pieces.
    Empty,
    /// The first piece is typed text starting with `.`, which would hide the
    /// file.
    StartsWithDot,
    /// Typed text holds this character.
    ForbiddenCharacter(char),
}

/// A name pattern: pieces in order, each a value or typed text. A save fills
/// each value and keeps the text as typed (see [`Self::fill`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NamePattern<Value>(Vec<NamePiece<Value>>);

impl<Value> NamePattern<Value> {
    pub fn new(pieces: Vec<NamePiece<Value>>) -> Self {
        Self(pieces)
    }

    pub fn pieces(&self) -> &[NamePiece<Value>] {
        &self.0
    }

    pub fn into_pieces(self) -> Vec<NamePiece<Value>> {
        self.0
    }

    /// Whether the pattern can make a name, and if not, why.
    pub fn validate(&self) -> Result<(), NamePatternError> {
        if self.0.is_empty() {
            return Err(NamePatternError::Empty);
        }
        for (index, piece) in self.0.iter().enumerate() {
            let NamePiece::Text(text) = piece else { continue };
            if index == 0 && text.starts_with('.') {
                return Err(NamePatternError::StartsWithDot);
            }
            if let Some(character) = text.chars().find(|character| FORBIDDEN.contains(character)) {
                return Err(NamePatternError::ForbiddenCharacter(character));
            }
        }
        Ok(())
    }

    /// Whether some piece is typed text of nothing, which a pattern never
    /// stores: typing nothing adds no piece.
    pub fn has_empty_text(&self) -> bool {
        self.0.iter().any(|piece| matches!(piece, NamePiece::Text(text) if text.is_empty()))
    }

    /// The pattern's text with `value_of` filled in for each value: no
    /// spaces added between pieces, none collapsed, and a value that is
    /// empty leaves only its neighbours' text.
    pub fn fill(&self, value_of: impl Fn(&Value) -> String) -> String {
        self.0
            .iter()
            .map(|piece| match piece {
                NamePiece::Value(value) => value_of(value),
                NamePiece::Text(text) => text.clone(),
            })
            .collect()
    }
}

#[cfg(test)]
#[path = "name_pattern_tests.rs"]
mod tests;
