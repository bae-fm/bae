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
pub use medium::Medium;
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

/// A release's pressing-level editorial metadata: its identifiers and year,
/// and what it is. `Pressing::blank()` is "no pressing claim".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pressing {
    /// Release-specific year (may differ from album year)
    pub year: Option<i32>,
    pub label: Option<String>,
    pub catalog_number: Option<String>,
    pub barcode: Option<String>,
    pub facts: PressingFacts,
}

impl Pressing {
    /// Nothing stated — "user claimed an album, not a specific pressing."
    /// Used when import identity is Approximate.
    pub fn blank() -> Self {
        Self {
            year: None,
            label: None,
            catalog_number: None,
            barcode: None,
            facts: PressingFacts::default(),
        }
    }

    /// This pressing, with each field `self` leaves unstated taken from
    /// `other`.
    pub fn fill_missing(&mut self, other: Self) {
        self.year = self.year.or(other.year);
        self.label = self.label.take().or(other.label);
        self.catalog_number = self.catalog_number.take().or(other.catalog_number);
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
