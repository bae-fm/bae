//! What a credit says the artist did, in bae's own vocabulary.
//!
//! MusicBrainz and Discogs state a credit's role in their own words. Each
//! catalog's reader turns a role into a [`CreditedAs`] once: a role bae
//! names ([`CreditRoleKind`]), or the catalog's own words where bae has no
//! name for it, with the qualifiers the catalog states beside it. The writing
//! roles are the ones that make an artist a composer of the work.
//!
//! Every closed list a catalog states from is reproduced beside the value it
//! names, name for name, with a test comparing the table against the captured
//! page under `test-fixtures/credit-vocabulary/`.

pub mod discogs;
pub mod musicbrainz;

use crate::pressing::vocabulary;

vocabulary! {
    /// A role bae names. The first eight are the writing roles: MusicBrainz's
    /// own `writer` family, the roles that make an artist a composer.
    pub enum CreditRoleKind {
        Writer => "writer",
        Composer => "composer",
        Lyricist => "lyricist",
        Librettist => "librettist",
        Translator => "translator",
        Reviser => "reviser",
        Scriptwriter => "scriptwriter",
        Reconstructor => "reconstructor",
        Arranger => "arranger",
        Producer => "producer",
        Engineer => "engineer",
        Mixer => "mixer",
        Mastering => "mastering",
    }
}

impl CreditRoleKind {
    /// Whether the role is one of the writing roles.
    pub fn is_writing(self) -> bool {
        matches!(
            self,
            Self::Writer
                | Self::Composer
                | Self::Lyricist
                | Self::Librettist
                | Self::Translator
                | Self::Reviser
                | Self::Scriptwriter
                | Self::Reconstructor
        )
    }

    /// Every writing role, in declaration order.
    pub fn writing() -> impl Iterator<Item = Self> {
        Self::ALL.iter().copied().filter(|kind| kind.is_writing())
    }

    /// The writing roles' keys as a SQL list, `'writer', 'composer', …`, to
    /// stand between the parentheses of an `IN`.
    pub fn writing_keys_sql() -> String {
        Self::writing().map(|kind| format!("'{}'", kind.key())).collect::<Vec<_>>().join(", ")
    }
}

vocabulary! {
    /// A modifier a catalog states beside a role, which bae names.
    pub enum QualifierKind {
        Additional => "additional",
        Assistant => "assistant",
        Associate => "associate",
        Co => "co",
        Executive => "executive",
        Guest => "guest",
        Solo => "solo",
        Pre => "pre",
        Re => "re",
    }
}

/// What an artist did on a work, release or track: a role bae names, or the
/// catalog's words for a role it has no name for.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CreditRole {
    Kind(CreditRoleKind),
    /// A performer, with the instrument or voice the catalog names; `None`
    /// where it names none.
    Performer(Option<String>),
    Stated(String),
}

/// A qualifier of a role: a modifier bae names, or the catalog's words.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoleQualifier {
    Kind(QualifierKind),
    Stated(String),
}

/// A credit's role with its qualifiers, in the order the catalog states them.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CreditedAs {
    pub role: CreditRole,
    pub qualifiers: Vec<RoleQualifier>,
}

/// The key a stored role column holds for a role in the catalog's words.
pub const STATED_ROLE_KEY: &str = "stated";

/// The key a stored role column holds for a performer; its words are the
/// instrument or voice, if the catalog names one.
pub const PERFORMER_ROLE_KEY: &str = "performer";

/// A [`CreditedAs`] as the three columns of a credit's row hold it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreditColumns {
    /// A [`CreditRoleKind`] key, or [`STATED_ROLE_KEY`].
    pub role: &'static str,
    /// The catalog's words, for a stated role, and for a performer the
    /// instrument or voice it names.
    pub role_words: Option<String>,
    /// The qualifiers as a JSON array.
    pub qualifiers: String,
}

/// Why a stored credit does not read as a [`CreditedAs`].
#[derive(Debug, thiserror::Error)]
pub enum CreditColumnsError {
    #[error("{0:?} is not a credit role")]
    UnknownRole(String),
    #[error("a stated credit role holds no words")]
    StatedWithoutWords,
    #[error("a credit role named by key {0:?} holds words")]
    WordsWithKey(String),
    #[error("the qualifiers {0:?} are not a list of qualifiers: {1}")]
    Qualifiers(String, serde_json::Error),
}

impl CreditedAs {
    /// A credit in role `kind` with no qualifiers.
    pub fn of(kind: CreditRoleKind) -> Self {
        Self { role: CreditRole::Kind(kind), qualifiers: Vec::new() }
    }

    /// The columns a row holds this credit as.
    pub fn columns(&self) -> CreditColumns {
        let (role, role_words) = match &self.role {
            CreditRole::Kind(kind) => (kind.key(), None),
            CreditRole::Performer(part) => (PERFORMER_ROLE_KEY, part.clone()),
            CreditRole::Stated(words) => (STATED_ROLE_KEY, Some(words.clone())),
        };
        CreditColumns {
            role,
            role_words,
            qualifiers: serde_json::to_string(&self.qualifiers)
                .expect("a list of qualifiers is JSON"),
        }
    }

    /// The credit a row's columns hold.
    pub fn from_columns(
        role: &str,
        role_words: Option<String>,
        qualifiers: &str,
    ) -> Result<Self, CreditColumnsError> {
        let role = match (role, role_words) {
            (STATED_ROLE_KEY, Some(words)) => CreditRole::Stated(words),
            (STATED_ROLE_KEY, None) => return Err(CreditColumnsError::StatedWithoutWords),
            (PERFORMER_ROLE_KEY, part) => CreditRole::Performer(part),
            (key, None) => CreditRole::Kind(
                CreditRoleKind::from_key(key)
                    .ok_or_else(|| CreditColumnsError::UnknownRole(key.to_string()))?,
            ),
            (key, Some(_)) => return Err(CreditColumnsError::WordsWithKey(key.to_string())),
        };
        let qualifiers = serde_json::from_str(qualifiers)
            .map_err(|error| CreditColumnsError::Qualifiers(qualifiers.to_string(), error))?;
        Ok(Self { role, qualifiers })
    }
}

#[cfg(test)]
#[path = "credit_role_tests.rs"]
mod tests;
