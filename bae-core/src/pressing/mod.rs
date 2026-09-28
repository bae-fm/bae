//! What a pressing is, in bae's own vocabulary.
//!
//! MusicBrainz and Discogs describe a release's place, media, status and
//! packaging in their own words: a country as a code or a name, a medium as
//! one format per disc or as a flat list of names and qualifiers. Both are
//! read here, once, into typed values — [`PressingFacts`] — so nothing past
//! the catalog boundary carries a catalog's raw text, and a surface renders
//! one shape whichever catalog stated it.
//!
//! Every closed list a catalog states from is reproduced in the module that
//! reads it, name for name, with a test comparing the table against the
//! captured record under `test-fixtures/pressing-vocabulary/`.

pub mod area;
pub mod country;
pub mod discogs_detail;
mod label_lines;
pub mod medium;
pub mod packaging;
pub mod stated_media;
pub mod status;

desktop_only! {
    pub(crate) mod discogs_formats;
    pub(crate) mod musicbrainz;
}

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

    /// These facts, with each part `self` leaves unstated taken from `other`.
    pub fn fill_missing(&mut self, other: Self) {
        self.area = self.area.or(other.area);
        if self.media.is_empty() {
            self.media = other.media;
        }
        self.status = self.status.or(other.status);
        self.packaging = self.packaging.or(other.packaging);
        if self.discogs_details.is_empty() {
            self.discogs_details = other.discogs_details;
        }
    }

    /// The physical carrier a player pauses between the sides or discs of:
    /// the first of the pressing's media that has one.
    pub fn physical_medium(&self) -> Option<PhysicalMedium> {
        self.media
            .iter()
            .find_map(|counted| counted.medium.physical())
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
/// placeholder for "no number" is no number.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize)]
pub struct ReleaseLabel {
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    catalog_number: Option<String>,
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
        })
    }

    /// The labels `entries` state, in order, each once.
    pub fn list(entries: impl IntoIterator<Item = (Option<String>, Option<String>)>) -> Vec<Self> {
        let mut labels: Vec<Self> = Vec::new();
        for label in entries
            .into_iter()
            .filter_map(|(name, catalog_number)| Self::new(name, catalog_number))
        {
            if !labels.contains(&label) {
                labels.push(label);
            }
        }
        labels
    }

    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    pub fn catalog_number(&self) -> Option<&str> {
        self.catalog_number.as_deref()
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

/// Read through [`ReleaseLabel::new`], so an entry stating neither half fails.
impl<'de> serde::Deserialize<'de> for ReleaseLabel {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Stated {
            #[serde(default)]
            name: Option<String>,
            #[serde(default)]
            catalog_number: Option<String>,
        }
        let Stated {
            name,
            catalog_number,
        } = Stated::deserialize(deserializer)?;
        Self::new(name, catalog_number)
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
    pub barcode: Option<String>,
    pub facts: PressingFacts,
}

impl Pressing {
    /// Nothing stated — "user claimed an album, not a specific pressing."
    /// Used when import identity is Approximate.
    pub fn blank() -> Self {
        Self {
            year: None,
            labels: Vec::new(),
            barcode: None,
            facts: PressingFacts::default(),
        }
    }

    /// This pressing, with each field `self` leaves unstated taken from
    /// `other`. The labels are taken whole, so no name is paired with another
    /// source's number.
    pub fn fill_missing(&mut self, other: Self) {
        self.year = self.year.or(other.year);
        if self.labels.is_empty() {
            self.labels = other.labels;
        }
        self.barcode = self.barcode.take().or(other.barcode);
        self.facts.fill_missing(other.facts);
    }
}

/// Whether two names are the same word. Compared character by character in
/// lower case, so neither side is allocated and the accented names compare
/// the way the unaccented ones do.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn same_name(a: &str, b: &str) -> bool {
    a.chars()
        .flat_map(char::to_lowercase)
        .eq(b.chars().flat_map(char::to_lowercase))
}

/// The names of a captured page: one per line, with the line naming the
/// source left out.
#[cfg(test)]
fn recorded(page: &str) -> Vec<&str> {
    page.lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect()
}

/// The area a MusicBrainz release-country code names, for a test that states
/// one.
#[cfg(test)]
pub(crate) fn area(code: &str) -> ReleaseArea {
    ReleaseArea::musicbrainz(code).unwrap_or_else(|| panic!("{code} names an area"))
}

/// Facts stating only that the pressing is `count` of `medium`, for a test
/// that states media.
#[cfg(test)]
pub(crate) fn made_of(medium: Medium, count: u32) -> PressingFacts {
    PressingFacts {
        media: vec![MediaCount { medium, count }],
        ..PressingFacts::default()
    }
}
