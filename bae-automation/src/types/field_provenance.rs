//! Where each of a release's album-level fields came from, and what the
//! catalogs describing it say about them.

use super::*;

/// Where one album-level field's value came from.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutomationFieldOrigin {
    /// A catalog's description of the release.
    Record { catalog: String },
    /// The audio files' own tags.
    Tags,
    /// A person typed it.
    Typed,
}

/// What one catalog's record of the release says about one field. `null` when
/// that record states nothing for it.
#[derive(Debug, Clone, Serialize)]
pub struct AutomationFieldClaim {
    pub catalog: String,
    pub value: Option<String>,
}

/// One field's whole story: where its value came from, what every catalog
/// describing the release says about it, and whether they disagree.
#[derive(Debug, Clone, Serialize)]
pub struct AutomationFieldProvenance {
    pub field: String,
    pub origin: Option<AutomationFieldOrigin>,
    pub claims: Vec<AutomationFieldClaim>,
    /// True when two catalogs state different things for this field.
    pub records_disagree: bool,
}
