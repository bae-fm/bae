//! The name a person gives what they make or rename: a library, a profile,
//! a playlist, another printed name of an artist.

/// A name as a person entered it, trimmed, and never blank. Every write that
/// takes a name from a person takes this type, so the rule is stated once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnteredName(String);

/// A name was empty, or all whitespace, once trimmed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("a name cannot be blank")]
pub struct BlankName;

impl EnteredName {
    /// `raw` trimmed, or [`BlankName`] when nothing is left.
    pub fn parse(raw: &str) -> Result<Self, BlankName> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            Err(BlankName)
        } else {
            Ok(Self(trimmed.to_string()))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

#[cfg(test)]
#[path = "entered_name_tests.rs"]
mod tests;
