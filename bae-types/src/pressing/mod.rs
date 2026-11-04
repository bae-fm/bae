//! What a pressing is, in bae's own vocabulary.
//!
//! MusicBrainz and Discogs describe a release's place, media, status and
//! packaging in their own words: a country as a code or a name, a medium as
//! one format per disc or as a flat list of names and qualifiers. Each
//! catalog's reader turns a record into typed values — [`PressingFacts`] —
//! once, by the names this module knows, so nothing past the catalog boundary
//! carries a catalog's raw text, and a surface renders one shape whichever
//! catalog stated it.
//!
//! Every closed list a catalog states from is reproduced beside the value it
//! names, name for name, with a test comparing the table against the captured
//! record under `test-fixtures/pressing-vocabulary/`.

pub mod area;
pub mod country;
pub mod discogs_detail;
mod label_lines;
pub mod medium;
pub mod packaging;
pub mod stated_media;
pub mod status;

pub use area::{Region, ReleaseArea};
pub use country::Country;
pub use discogs_detail::DiscogsDetail;
pub use label_lines::{label_lines, LabelLine};
pub use medium::{CdAudio, Medium};
pub use packaging::Packaging;
pub use stated_media::{StatedFormat, StatedMedia};
pub use status::ReleaseStatus;

/// A closed vocabulary: an enum whose every variant has the key bae stores
/// and serializes it as. The key never changes once written — it is what the
/// database and the wire hold — and a key outside the list decodes to
/// nothing, which the reader reports rather than guesses at.
macro_rules! vocabulary {
    (
        $(#[$meta:meta])*
        pub enum $name:ident {
            $($(#[$vmeta:meta])* $variant:ident => $key:literal),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum $name {
            $($(#[$vmeta])* $variant),+
        }

        impl $name {
            /// Every value, in declaration order.
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];

            /// The key this value is stored and serialized as.
            pub fn key(self) -> &'static str {
                match self {
                    $(Self::$variant => $key),+
                }
            }

            /// The value a stored key names, `None` for a key outside the list.
            pub fn from_key(key: &str) -> Option<Self> {
                match key {
                    $($key => Some(Self::$variant),)+
                    _ => None,
                }
            }
        }

        impl serde::Serialize for $name {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(self.key())
            }
        }

        impl<'de> serde::Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let key = <std::borrow::Cow<'de, str>>::deserialize(deserializer)?;
                Self::from_key(&key).ok_or_else(|| {
                    serde::de::Error::custom(format!(
                        concat!("{:?} is not a ", stringify!($name)),
                        key
                    ))
                })
            }
        }
    };
}
pub(crate) use vocabulary;

/// How many of one carrier a pressing holds: two CDs, one vinyl record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct MediaCount {
    pub medium: Medium,
    pub count: u32,
}

impl MediaCount {
    /// The carriers `media` name, each with how many of it there are, in the
    /// order each carrier first appears. `media` names one carrier per entry,
    /// with a quantity: a MusicBrainz medium is one of its carrier, a Discogs
    /// format entry states its own quantity.
    pub fn tally(media: impl IntoIterator<Item = (Medium, u32)>) -> Vec<Self> {
        let mut tally: Vec<Self> = Vec::new();
        for (medium, count) in media {
            match tally.iter_mut().find(|counted| counted.medium == medium) {
                Some(counted) => counted.count += count,
                None => tally.push(Self { medium, count }),
            }
        }
        tally
    }
}

/// What a pressing is: where it was released, what it is made of, how
/// official it is, what it is sold in, and the details only Discogs states
/// about it. Every part may be unstated; `PressingFacts::default()` states
/// nothing.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct PressingFacts {
    pub area: Option<ReleaseArea>,
    /// Each carrier with its count, in the order the record lists them.
    pub media: Vec<MediaCount>,
    pub status: Option<ReleaseStatus>,
    pub packaging: Option<Packaging>,
    /// What Discogs says about the pressing that neither catalog has a field
    /// for — a reissue, a remaster, a limited edition — in the order it
    /// states them, each once.
    pub discogs_details: Vec<DiscogsDetail>,
}

impl PressingFacts {
    /// Whether nothing is stated.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// A physical carrier whose side or disc boundaries can pause playback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhysicalMedium {
    /// A grooved disc played one side at a time.
    Record,
    Cassette,
    Cd,
}

/// One label a pressing is released on, with the catalog number that label
/// gives it. Either half may be unstated, never both, and a catalog's
/// placeholder for "no number" is no number. A named label may carry the ids
/// the catalogs know it by: what decides which library label it is.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize)]
pub struct ReleaseLabel {
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    catalog_number: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    musicbrainz_label_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    discogs_label_id: Option<String>,
}

impl ReleaseLabel {
    /// `None` when neither half is stated. A blank name is unstated, and so
    /// is a number `catalog_key` reads as none.
    pub fn new(name: Option<String>, catalog_number: Option<String>) -> Option<Self> {
        let name = name.filter(|name| !name.trim().is_empty());
        let catalog_number =
            catalog_number.filter(|number| crate::text_match::catalog_key(number).is_some());
        (name.is_some() || catalog_number.is_some()).then_some(Self {
            name,
            catalog_number,
            musicbrainz_label_id: None,
            discogs_label_id: None,
        })
    }

    /// The label with the ids the catalogs know it by. A label stating no
    /// name keeps none: an id names a label, and a label is known by its name.
    #[must_use]
    pub fn with_ids(
        mut self,
        musicbrainz_label_id: Option<String>,
        discogs_label_id: Option<String>,
    ) -> Self {
        let named = self.name.is_some();
        let id = |id: Option<String>| id.filter(|id| named && !id.trim().is_empty());
        self.musicbrainz_label_id = id(musicbrainz_label_id);
        self.discogs_label_id = id(discogs_label_id);
        self
    }

    /// The labels `entries` state, in order, each once.
    pub fn list(entries: impl IntoIterator<Item = (Option<String>, Option<String>)>) -> Vec<Self> {
        Self::each_once(
            entries
                .into_iter()
                .filter_map(|(name, catalog_number)| Self::new(name, catalog_number)),
        )
    }

    /// `labels` in order, each once.
    pub fn each_once(labels: impl IntoIterator<Item = Self>) -> Vec<Self> {
        let mut once: Vec<Self> = Vec::new();
        for label in labels {
            if !once.contains(&label) {
                once.push(label);
            }
        }
        once
    }

    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    pub fn catalog_number(&self) -> Option<&str> {
        self.catalog_number.as_deref()
    }

    pub fn musicbrainz_label_id(&self) -> Option<&str> {
        self.musicbrainz_label_id.as_deref()
    }

    pub fn discogs_label_id(&self) -> Option<&str> {
        self.discogs_label_id.as_deref()
    }

    /// Whether the two state one label: names of one label
    /// ([`same_label_name`](crate::text_match::same_label_name)), or neither
    /// naming one, and numbers that compare as one
    /// ([`catalog_key`](crate::text_match::catalog_key)), or neither stating
    /// one.
    pub fn same_label(&self, other: &Self) -> bool {
        let same_name = match (self.name(), other.name()) {
            (Some(a), Some(b)) => crate::text_match::same_label_name(a, b),
            (a, b) => a.is_none() && b.is_none(),
        };
        let key = |label: &Self| label.catalog_number().and_then(crate::text_match::catalog_key);
        same_name && key(self) == key(other)
    }

    /// Whether the two print one catalog number under one label: both name
    /// a label and state a number, and they state [one label](Self::same_label).
    pub fn prints_number_of(&self, other: &Self) -> bool {
        self.name.is_some()
            && other.name.is_some()
            && self.catalog_number.is_some()
            && self.same_label(other)
    }

    /// Fill each id this label does not carry from `other`, a statement of
    /// the same label by another record.
    pub fn take_missing_ids(&mut self, other: &Self) {
        if self.name.is_none() {
            return;
        }
        if self.musicbrainz_label_id.is_none() {
            self.musicbrainz_label_id.clone_from(&other.musicbrainz_label_id);
        }
        if self.discogs_label_id.is_none() {
            self.discogs_label_id.clone_from(&other.discogs_label_id);
        }
    }
}

#[cfg(any(test, feature = "test-utils"))]
impl ReleaseLabel {
    /// A label for a test; `name` and `catalog_number` are not both `None`.
    pub fn of(name: Option<&str>, catalog_number: Option<&str>) -> Self {
        Self::new(name.map(str::to_string), catalog_number.map(str::to_string))
            .expect("a test's label states a name or a number")
    }
}

/// Read through [`ReleaseLabel::new`] and [`ReleaseLabel::with_ids`], so an
/// entry stating neither half fails.
impl<'de> serde::Deserialize<'de> for ReleaseLabel {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Stated {
            #[serde(default)]
            name: Option<String>,
            #[serde(default)]
            catalog_number: Option<String>,
            #[serde(default)]
            musicbrainz_label_id: Option<String>,
            #[serde(default)]
            discogs_label_id: Option<String>,
        }
        let Stated { name, catalog_number, musicbrainz_label_id, discogs_label_id } =
            Stated::deserialize(deserializer)?;
        Self::new(name, catalog_number)
            .map(|label| label.with_ids(musicbrainz_label_id, discogs_label_id))
            .ok_or_else(|| serde::de::Error::custom("a label states neither a name nor a number"))
    }
}

/// A release's pressing-level editorial metadata: its identifiers and year,
/// and what it is. `Pressing::blank()` is "no pressing claim".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pressing {
    /// Release-specific year (may differ from album year)
    pub year: Option<i32>,
    /// Every label the pressing is on, in the order its source lists them.
    pub labels: Vec<ReleaseLabel>,
    /// Every barcode the pressing is printed with, in the order its source
    /// lists them, each once: a release can carry an EAN and a UPC.
    pub barcodes: Vec<String>,
    pub facts: PressingFacts,
}

impl Pressing {
    /// Nothing stated — "user claimed an album, not a specific pressing."
    /// Used when import identity is Approximate.
    pub fn blank() -> Self {
        Self {
            year: None,
            labels: Vec::new(),
            barcodes: Vec::new(),
            facts: PressingFacts::default(),
        }
    }
}

/// Whether two names are the same word. Compared character by character in
/// lower case, so neither side is allocated and the accented names compare
/// the way the unaccented ones do.
#[cfg(desktop)]
fn same_name(a: &str, b: &str) -> bool {
    a.chars().flat_map(char::to_lowercase).eq(b.chars().flat_map(char::to_lowercase))
}

/// The names of a captured page: one per line, with the line naming the
/// source left out.
#[cfg(test)]
fn recorded(page: &str) -> Vec<&str> {
    page.lines().filter(|line| !line.is_empty() && !line.starts_with('#')).collect()
}

/// The area a MusicBrainz release-country code names, for a test that states
/// one.
#[cfg(any(test, feature = "test-utils"))]
pub fn area(code: &str) -> ReleaseArea {
    ReleaseArea::musicbrainz(code).unwrap_or_else(|| panic!("{code} names an area"))
}

/// Facts stating only that the pressing is `count` of `medium`, for a test
/// that states media.
#[cfg(any(test, feature = "test-utils"))]
pub fn made_of(medium: Medium, count: u32) -> PressingFacts {
    PressingFacts { media: vec![MediaCount { medium, count }], ..PressingFacts::default() }
}
