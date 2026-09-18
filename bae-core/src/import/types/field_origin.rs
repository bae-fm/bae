//! Where each of a draft's album-level fields came from, what the catalogs
//! describing the release say about them, and what that makes worth pointing
//! at.
//!
//! A value and where it was read are one fact, so the origins travel on the
//! draft beside the values they describe. What a catalog says about a field is
//! not stored at all: it is read back from the archived documents whenever a
//! surface asks.

use super::*;
use std::borrow::Cow;

/// One album-level field of the metadata form.
///
/// The form's own fields, not the wire edit's: `year` is text here because the
/// field is text, and the commit is what parses it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CandidateEditField {
    AlbumTitle,
    AlbumYear,
    PressingYear,
    Format,
    Label,
    CatalogNumber,
    Country,
    Barcode,
}

impl CandidateEditField {
    /// Every album-level field, in the order the form lists them.
    pub const ALL: [Self; 8] = [
        Self::AlbumTitle,
        Self::AlbumYear,
        Self::PressingYear,
        Self::Format,
        Self::Label,
        Self::CatalogNumber,
        Self::Country,
        Self::Barcode,
    ];

    /// The field's stable name, for a surface that names fields rather than
    /// drawing them.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AlbumTitle => "album_title",
            Self::AlbumYear => "album_year",
            Self::PressingYear => "pressing_year",
            Self::Format => "format",
            Self::Label => "label",
            Self::CatalogNumber => "catalog_number",
            Self::Country => "country",
            Self::Barcode => "barcode",
        }
    }

    /// Put `value` in this field of `draft`, which makes the value the
    /// person's own. Restating what the field already holds decides nothing
    /// and leaves its origin alone; emptying it leaves no value for an origin
    /// to describe.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub(crate) fn set<Track>(self, draft: &mut RawReleaseEditOf<Track>, value: &str) {
        if self.slot(draft) == value {
            return;
        }
        *self.slot_mut(draft) = value.to_string();
        let origin = (!value.trim().is_empty()).then_some(FieldOrigin::Typed);
        draft.origins.set(self, origin);
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub(crate) fn slot<Track>(self, draft: &RawReleaseEditOf<Track>) -> &str {
        match self {
            Self::AlbumTitle => &draft.album_title,
            Self::AlbumYear => &draft.album_year,
            Self::PressingYear => &draft.pressing.year,
            Self::Format => &draft.pressing.format,
            Self::Label => &draft.pressing.label,
            Self::CatalogNumber => &draft.pressing.catalog_number,
            Self::Country => &draft.pressing.country,
            Self::Barcode => &draft.pressing.barcode,
        }
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub(crate) fn slot_mut<Track>(self, draft: &mut RawReleaseEditOf<Track>) -> &mut String {
        match self {
            Self::AlbumTitle => &mut draft.album_title,
            Self::AlbumYear => &mut draft.album_year,
            Self::PressingYear => &mut draft.pressing.year,
            Self::Format => &mut draft.pressing.format,
            Self::Label => &mut draft.pressing.label,
            Self::CatalogNumber => &mut draft.pressing.catalog_number,
            Self::Country => &mut draft.pressing.country,
            Self::Barcode => &mut draft.pressing.barcode,
        }
    }
}

/// Where one album-level field's value came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FieldOrigin {
    /// A catalog's description of the release.
    Record(Catalog),
    /// The audio files' own tags.
    Tags,
    /// A person typed it.
    Typed,
}

impl FieldOrigin {
    /// The stored column value.
    pub fn as_str(&self) -> Cow<'static, str> {
        match self {
            Self::Record(catalog) => Cow::Owned(format!("record:{}", catalog.as_str())),
            Self::Tags => Cow::Borrowed("tags"),
            Self::Typed => Cow::Borrowed("typed"),
        }
    }
}

impl std::str::FromStr for FieldOrigin {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "tags" => Ok(Self::Tags),
            "typed" => Ok(Self::Typed),
            other => match other.strip_prefix("record:") {
                Some(catalog) => catalog.parse().map(Self::Record),
                None => Err(format!("unknown field origin: {other}")),
            },
        }
    }
}

impl std::fmt::Display for FieldOrigin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.as_str())
    }
}

/// Where every album-level field of one draft came from. `None` is a blank
/// field: an absent value came from nowhere.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldOrigins {
    pub album_title: Option<FieldOrigin>,
    pub album_year: Option<FieldOrigin>,
    pub pressing_year: Option<FieldOrigin>,
    pub format: Option<FieldOrigin>,
    pub label: Option<FieldOrigin>,
    pub catalog_number: Option<FieldOrigin>,
    pub country: Option<FieldOrigin>,
    pub barcode: Option<FieldOrigin>,
}

impl FieldOrigins {
    /// One source filled the whole form: `origin` for every field it states,
    /// and nothing for every field it leaves blank.
    pub fn of(values: &FieldValues, origin: FieldOrigin) -> Self {
        let mut origins = Self::default();
        for field in CandidateEditField::ALL {
            if !values.get(field).trim().is_empty() {
                origins.set(field, Some(origin));
            }
        }
        origins
    }

    pub fn get(&self, field: CandidateEditField) -> Option<FieldOrigin> {
        *self.slot(field)
    }

    pub fn set(&mut self, field: CandidateEditField, origin: Option<FieldOrigin>) {
        *self.slot_mut(field) = origin;
    }

    fn slot(&self, field: CandidateEditField) -> &Option<FieldOrigin> {
        match field {
            CandidateEditField::AlbumTitle => &self.album_title,
            CandidateEditField::AlbumYear => &self.album_year,
            CandidateEditField::PressingYear => &self.pressing_year,
            CandidateEditField::Format => &self.format,
            CandidateEditField::Label => &self.label,
            CandidateEditField::CatalogNumber => &self.catalog_number,
            CandidateEditField::Country => &self.country,
            CandidateEditField::Barcode => &self.barcode,
        }
    }

    fn slot_mut(&mut self, field: CandidateEditField) -> &mut Option<FieldOrigin> {
        match field {
            CandidateEditField::AlbumTitle => &mut self.album_title,
            CandidateEditField::AlbumYear => &mut self.album_year,
            CandidateEditField::PressingYear => &mut self.pressing_year,
            CandidateEditField::Format => &mut self.format,
            CandidateEditField::Label => &mut self.label,
            CandidateEditField::CatalogNumber => &mut self.catalog_number,
            CandidateEditField::Country => &mut self.country,
            CandidateEditField::Barcode => &mut self.barcode,
        }
    }
}

/// The eight album-level fields of a release as text, empty meaning "states
/// nothing" — the form's own shape, without the artists and tracks the same
/// form carries.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FieldValues {
    pub album_title: String,
    pub album_year: String,
    pub pressing_year: String,
    pub format: String,
    pub label: String,
    pub catalog_number: String,
    pub country: String,
    pub barcode: String,
}

impl FieldValues {
    /// What a draft states.
    pub fn of_draft<Track>(draft: &RawReleaseEditOf<Track>) -> Self {
        Self {
            album_title: draft.album_title.clone(),
            album_year: draft.album_year.clone(),
            pressing_year: draft.pressing.year.clone(),
            format: draft.pressing.format.clone(),
            label: draft.pressing.label.clone(),
            catalog_number: draft.pressing.catalog_number.clone(),
            country: draft.pressing.country.clone(),
            barcode: draft.pressing.barcode.clone(),
        }
    }

    /// What a wire edit states, rendered as the form holds it.
    pub fn of_edit(edit: &ReleaseUserEdit) -> Self {
        let text = |value: &Option<String>| value.clone().unwrap_or_default();
        let year = |value: Option<i32>| value.map(|y| y.to_string()).unwrap_or_default();
        Self {
            album_title: edit.album_title.clone(),
            album_year: year(edit.album_year),
            pressing_year: year(edit.pressing.year),
            format: text(&edit.pressing.format),
            label: text(&edit.pressing.label),
            catalog_number: text(&edit.pressing.catalog_number),
            country: text(&edit.pressing.country),
            barcode: text(&edit.pressing.barcode),
        }
    }

    pub fn set(&mut self, field: CandidateEditField, value: &str) {
        *self.slot_mut(field) = value.to_string();
    }

    /// Put these values into a wire edit. A year that does not read as a
    /// number is "not set", which is what a blank year means.
    pub fn apply_to(&self, edit: &mut ReleaseUserEdit) {
        let stated = |value: &str| {
            let value = value.trim();
            (!value.is_empty()).then(|| value.to_string())
        };
        let year = |value: &str| value.trim().parse::<i32>().ok();
        edit.album_title = self.album_title.clone();
        edit.album_year = year(&self.album_year);
        edit.pressing.year = year(&self.pressing_year);
        edit.pressing.format = stated(&self.format);
        edit.pressing.label = stated(&self.label);
        edit.pressing.catalog_number = stated(&self.catalog_number);
        edit.pressing.country = stated(&self.country);
        edit.pressing.barcode = stated(&self.barcode);
    }

    pub fn get(&self, field: CandidateEditField) -> &str {
        match field {
            CandidateEditField::AlbumTitle => &self.album_title,
            CandidateEditField::AlbumYear => &self.album_year,
            CandidateEditField::PressingYear => &self.pressing_year,
            CandidateEditField::Format => &self.format,
            CandidateEditField::Label => &self.label,
            CandidateEditField::CatalogNumber => &self.catalog_number,
            CandidateEditField::Country => &self.country,
            CandidateEditField::Barcode => &self.barcode,
        }
    }

    fn slot_mut(&mut self, field: CandidateEditField) -> &mut String {
        match field {
            CandidateEditField::AlbumTitle => &mut self.album_title,
            CandidateEditField::AlbumYear => &mut self.album_year,
            CandidateEditField::PressingYear => &mut self.pressing_year,
            CandidateEditField::Format => &mut self.format,
            CandidateEditField::Label => &mut self.label,
            CandidateEditField::CatalogNumber => &mut self.catalog_number,
            CandidateEditField::Country => &mut self.country,
            CandidateEditField::Barcode => &mut self.barcode,
        }
    }
}

/// What one catalog's record of the release says about one field. `None` when
/// that record states nothing for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldClaim {
    pub catalog: Catalog,
    pub value: Option<String>,
}

/// What every catalog describing a release says about its album-level fields:
/// one reading per catalog, each that catalog's own archived document
/// projected through the same mapping the draft is read with.
///
/// Never stored. A record whose documents are not archived contributes no
/// reading — there is nothing to read it from.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FieldClaims {
    readings: Vec<(Catalog, FieldValues)>,
}

impl FieldClaims {
    /// Order the readings the way every surface lists catalogs.
    pub fn of(mut readings: Vec<(Catalog, FieldValues)>) -> Self {
        readings.sort_by_key(|(catalog, _)| {
            Catalog::ALL
                .iter()
                .position(|listed| listed == catalog)
                .expect("every catalog is listed in Catalog::ALL")
        });
        Self { readings }
    }

    /// What each catalog states for `field`.
    pub fn claims(&self, field: CandidateEditField) -> Vec<FieldClaim> {
        self.readings
            .iter()
            .map(|(catalog, values)| {
                let value = values.get(field).trim();
                FieldClaim {
                    catalog: *catalog,
                    value: (!value.is_empty()).then(|| value.to_string()),
                }
            })
            .collect()
    }
}

/// What one field's dot says. A field with nothing to say carries no dot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldDot {
    /// A person typed this value.
    Typed,
    /// The catalogs describing this release do not agree on this field.
    Disagreement,
}

/// One field's whole story: where its value came from, what every catalog
/// describing the release says about it, and what its dot says.
///
/// A disagreement outranks a typed value: the alternatives are what the dot
/// invites a look at, and the hover names the typed value either way.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldProvenance {
    pub field: CandidateEditField,
    pub origin: Option<FieldOrigin>,
    pub claims: Vec<FieldClaim>,
    pub dot: Option<FieldDot>,
}

impl FieldProvenance {
    /// Every field's story, in the order the form lists them.
    pub fn of(origins: &FieldOrigins, claims: &FieldClaims) -> Vec<Self> {
        CandidateEditField::ALL
            .into_iter()
            .map(|field| Self::one(field, origins.get(field), claims.claims(field)))
            .collect()
    }

    /// This field as a person typing over it leaves it. A form held in a
    /// surface until it is saved has no stored origin to read yet, so the dot
    /// it draws while a person types is this one.
    pub fn typed(self) -> Self {
        Self::one(self.field, Some(FieldOrigin::Typed), self.claims)
    }

    /// Whether two catalogs state different things for this field.
    pub fn records_disagree(&self) -> bool {
        disagree(&self.claims)
    }

    fn one(
        field: CandidateEditField,
        origin: Option<FieldOrigin>,
        claims: Vec<FieldClaim>,
    ) -> Self {
        let dot = if disagree(&claims) {
            Some(FieldDot::Disagreement)
        } else if origin == Some(FieldOrigin::Typed) {
            Some(FieldDot::Typed)
        } else {
            None
        };
        Self {
            field,
            origin,
            claims,
            dot,
        }
    }
}

/// Whether two catalogs state different things for the field. A catalog that
/// states nothing disagrees with nobody.
fn disagree(claims: &[FieldClaim]) -> bool {
    let mut stated = claims.iter().filter_map(|claim| claim.value.as_deref());
    let Some(first) = stated.next() else {
        return false;
    };
    stated.any(|value| value != first)
}
