//! A credit list as it prints: every album's and track's artist line is built
//! here, from each credit's printed name and what its source prints after it.
//!
//! A line is kept as names with typed joins, not as one string: the apps word
//! a recognized join in the viewer's language and join a run of plain names
//! with their list formatter. Only a line that leaves bae with no language
//! (see [`CreditLine::plain_text`]) is a string.

use serde::{Deserialize, Serialize};

/// What follows a credit in a line, other than the plain list separator.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CreditJoin {
    /// " feat. "
    Featuring,
    /// " & "
    Ampersand,
    /// " and "
    And,
    /// " with "
    With,
    /// " vs. "
    Versus,
    /// A join bae does not recognize, shown as its source wrote it.
    Written(String),
}

impl CreditJoin {
    /// The join a stored join phrase is. `None` for a phrase that states
    /// nothing, or only the list separator (a comma): those are the plain
    /// join between names.
    pub fn from_stored(stored: &str) -> Option<Self> {
        let words = stored.trim();
        if words.is_empty() || words == "," {
            return None;
        }
        Some(match words.to_lowercase().as_str() {
            "feat." => Self::Featuring,
            "&" => Self::Ampersand,
            "and" => Self::And,
            "with" => Self::With,
            "vs." | "vs" => Self::Versus,
            _ => Self::Written(stored.to_string()),
        })
    }

    /// The join as English writes it, with the spaces either side: what a
    /// line that leaves bae with no language prints.
    pub fn english(&self) -> &str {
        match self {
            Self::Featuring => " feat. ",
            Self::Ampersand => " & ",
            Self::And => " and ",
            Self::With => " with ",
            Self::Versus => " vs. ",
            Self::Written(written) => written,
        }
    }
}

/// One credit of a line: its printed name, and the join that follows it
/// (`None` for the plain list separator, and for the last credit).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CreditedName {
    pub name: String,
    pub join: Option<CreditJoin>,
}

/// A credit list as it prints.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct CreditLine {
    names: Vec<CreditedName>,
}

impl CreditLine {
    /// The line of `credits`: each credit's printed name, and the join its
    /// source stored after it (`None` where it states none). The last
    /// credit's join is dropped: nothing follows it.
    pub fn from_stored<'a>(credits: impl IntoIterator<Item = (&'a str, Option<&'a str>)>) -> Self {
        let mut names: Vec<CreditedName> = credits
            .into_iter()
            .map(|(name, join)| CreditedName {
                name: name.to_string(),
                join: join.and_then(CreditJoin::from_stored),
            })
            .collect();
        if let Some(last) = names.last_mut() {
            last.join = None;
        }
        Self { names }
    }

    /// A line of one name.
    pub fn of_name(name: &str) -> Self {
        Self::from_stored([(name, None)])
    }

    pub fn names(&self) -> &[CreditedName] {
        &self.names
    }

    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }

    /// The line for a place that has no viewer language (AirPlay and DLNA
    /// metadata, Subsonic's `artist`): plain names joined with ", ", a
    /// recognized join as English writes it, any other as written.
    pub fn plain_text(&self) -> String {
        let mut text = String::new();
        for (index, credited) in self.names.iter().enumerate() {
            text.push_str(&credited.name);
            if index + 1 < self.names.len() {
                text.push_str(credited.join.as_ref().map_or(", ", CreditJoin::english));
            }
        }
        text
    }

    /// Each credit's printed name, in order: what a saved file's tags hold,
    /// one value each.
    pub fn name_values(&self) -> impl Iterator<Item = &str> {
        self.names.iter().map(|credited| credited.name.as_str())
    }
}

/// One credit of a line as bae stores it: the printed name, and the join
/// phrase its source states after it. The stored form of a line wherever a
/// table holds one (JSON), and what SQL builds a line from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredCredit {
    pub name: String,
    pub join: Option<String>,
}

impl CreditLine {
    /// The line a stored list prints.
    pub fn from_stored_credits(credits: &[StoredCredit]) -> Self {
        Self::from_stored(
            credits.iter().map(|credit| (credit.name.as_str(), credit.join.as_deref())),
        )
    }

    /// The line as it is stored: a recognized join in the words
    /// [`CreditJoin::english`] writes, which reads back as the same join.
    pub fn to_stored_credits(&self) -> Vec<StoredCredit> {
        self.names
            .iter()
            .map(|credited| StoredCredit {
                name: credited.name.clone(),
                join: credited.join.as_ref().map(|join| join.english().to_string()),
            })
            .collect()
    }
}

/// A join as bae keeps it: `None` for nothing stated, so the plain join
/// prints.
pub fn stated_join(join: Option<&str>) -> Option<String> {
    join.filter(|join| !join.is_empty()).map(str::to_string)
}

/// The joins of a credit list as stored: the last credit's join is dropped,
/// since nothing follows it, so removing the credit after another leaves that
/// one printing no join.
pub fn drop_last_join<T>(credits: &mut [T], join: impl Fn(&mut T) -> &mut Option<String>) {
    if let Some(last) = credits.last_mut() {
        *join(last) = None;
    }
}

#[cfg(test)]
#[path = "credits_tests.rs"]
mod tests;
