//! The sort keys names are ordered by, in the language of the device.
//!
//! A key is the bytes of a name's Unicode collation in a language
//! (tertiary strength, so names differing only in case have different keys):
//! comparing two keys byte by byte orders their names as a reader of that
//! language expects. The schema stores a key beside every listed name, and
//! lists order by it.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use icu_collator::options::{CollatorOptions, Strength};
use icu_collator::{Collator, CollatorBorrowed, CollatorPreferences};
use icu_locale_core::Locale;

/// The version of the collation data and algorithm the keys are written by.
/// A key is valid only for the data it was made with, so this changes
/// whenever a key for some name changes (a new `icu_collator`), and the
/// device then rewrites every stored key. `keys_are_the_version_they_claim`
/// fails when a key changes without it.
pub const DATA_VERSION: u32 = 1;

/// The separator written after each name's key in a list key: below every
/// byte a key holds, so a list that is a prefix of another comes first and
/// two lists compare name by name.
const NAME_END: u8 = 0;

/// What separates the names of a list in the text `NameKeys::list_key` reads.
pub const LIST_SEPARATOR: char = '\u{1f}';

/// A language that is not a language tag.
#[derive(Debug, thiserror::Error)]
#[error("{tag:?} is not a language tag")]
pub struct InvalidLanguage {
    pub tag: String,
    #[source]
    source: icu_locale_core::ParseError,
}

/// The collator of one language.
pub struct NameKeys {
    tag: String,
    collator: CollatorBorrowed<'static>,
}

/// The language that sorts names by the root order, for a device with none
/// set.
pub const ROOT_LANGUAGE: &str = "und";

/// A language tag checked to be one, in its canonical form: what a read that
/// orders names takes to order them in the device's language.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SortLanguage(String);

impl SortLanguage {
    /// The language `tag` names.
    pub fn parse(tag: &str) -> Result<Self, InvalidLanguage> {
        Ok(Self(NameKeys::for_language(tag)?.tag().to_string()))
    }

    /// The root order, the language of a device with none set.
    pub fn root() -> Self {
        Self(ROOT_LANGUAGE.to_string())
    }

    /// The canonical tag.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The collators made so far, by the language tag they were asked for as.
static COLLATORS: Mutex<Option<HashMap<String, Arc<NameKeys>>>> = Mutex::new(None);

impl NameKeys {
    /// The keys of the language `tag` (a BCP 47 tag; "und" is the root
    /// order). A language with no tailoring of its own sorts by the root
    /// order.
    pub fn for_language(tag: &str) -> Result<Arc<NameKeys>, InvalidLanguage> {
        let mut made = COLLATORS.lock().expect("collators lock poisoned");
        let made = made.get_or_insert_with(HashMap::new);
        if let Some(keys) = made.get(tag) {
            return Ok(Arc::clone(keys));
        }
        let locale = Locale::try_from_str(tag)
            .map_err(|source| InvalidLanguage { tag: tag.to_string(), source })?;
        let mut options = CollatorOptions::default();
        options.strength = Some(Strength::Tertiary);
        let collator = Collator::try_new(CollatorPreferences::from(&locale), options)
            .expect("the collation data is compiled in");
        let keys = Arc::new(NameKeys { tag: locale.to_string(), collator });
        made.insert(tag.to_string(), Arc::clone(&keys));
        Ok(keys)
    }

    /// The language's canonical tag.
    pub fn tag(&self) -> &str {
        &self.tag
    }

    /// `name`'s sort key.
    pub fn key(&self, name: &str) -> Vec<u8> {
        let mut key = Vec::new();
        let Ok(()) = self.collator.write_sort_key_to(name, &mut key);
        key
    }

    /// The sort key of a list of names, `names` joined by [`LIST_SEPARATOR`]:
    /// each name's key and a byte below every key's, so lists order name by
    /// name, and a list comes before every list that starts with it.
    pub fn list_key(&self, names: &str) -> Vec<u8> {
        let mut key = Vec::new();
        for name in names.split(LIST_SEPARATOR) {
            key.extend(self.key(name));
            key.push(NAME_END);
        }
        key
    }
}

#[cfg(test)]
#[path = "name_key_tests.rs"]
mod tests;
