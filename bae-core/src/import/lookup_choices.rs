//! What a person has decided one candidate's identification asks about, and
//! what it is to make of the answers.
//!
//! Extraction reads a folder's disc ID, its barcodes and its catalog numbers;
//! which of them a run actually looks up is a separate question, and the
//! answer is the person's. They uncheck a barcode that belongs to the box set
//! rather than the disc, or pick the one catalog number of thirty that names
//! this pressing — and that decision outlives the run they made it during.
//!
//! So it is stored with the candidate rather than held inside a running
//! reducer: a run reads it at its start, and every later run reads the same
//! value until the person changes it. Changing what a run looks up is what
//! starts the next one.

/// The words a person typed for the title search, in place of what the draft
/// calls the release. An album tag that carries the catalog number in
/// brackets searches for nothing; the person takes it out here and the run
/// searches by what is left.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchWords {
    pub album: String,
    /// Blank searches by the title alone.
    pub artist: String,
}

/// The signals one candidate's identification leaves out, the catalog numbers
/// it asks about, the words it searches by, and the ones it is to read
/// nothing into.
///
/// The default is what a candidate nobody has touched runs with: the disc ID
/// and the barcodes are asked about, no catalog number is — one number can
/// name thirty releases, so a number is looked up only once someone says it is
/// this disc's — the draft's own title is searched, and everything the
/// folder's text says counts.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LookupChoices {
    /// Whether the run leaves the candidate's disc ID out. One value, so one
    /// flag: a folder derives at most one disc ID.
    pub disc_id_excluded: bool,
    /// The barcode values the run leaves out — one of the two codes on a
    /// double sleeve, or every code the folder carries.
    ///
    /// A set, each value once, sorted, as `discounted_catalogs` is: nothing
    /// dispatches on their order, and a code is left out or it is not. A code
    /// in here is asked of no provider.
    pub excluded_barcodes: Vec<String>,
    /// The catalog numbers the run looks up, each on its own, in the order
    /// they were chosen — which is the order their lookups are dispatched and
    /// their results laid out.
    pub chosen_catalogs: Vec<String>,
    /// What the title search asks for, where the person typed it. `None`
    /// searches by what the draft calls the release.
    pub search_words: Option<SearchWords>,
    /// The catalog numbers the folder's own text carries that the person
    /// struck out: a result whose catalog number is one of them agrees with
    /// the text about nothing, however plainly the text prints it.
    ///
    /// A set, each value once: nothing dispatches on their order, and a value
    /// is struck out or it is not. Nothing here reaches a provider — striking
    /// a number out changes how the answers in hand are ranked, not what was
    /// asked for them. So it is a choice apart from `chosen_catalogs`: a
    /// number can be looked up and struck out at once, striking one out
    /// leaves the lookups as they were, and counting it again chooses
    /// nothing.
    pub discounted_catalogs: Vec<String>,
}

impl SearchWords {
    /// These words with their edges trimmed, or `None` for words that name
    /// no title — which is no words at all, and the draft's title stands.
    pub fn trimmed(self) -> Option<Self> {
        let album = self.album.trim();
        (!album.is_empty()).then(|| Self {
            album: album.to_string(),
            artist: self.artist.trim().to_string(),
        })
    }
}

impl LookupChoices {
    /// This value as it is stored: the typed words trimmed, and each chosen
    /// number chosen once however it is spelled. Numbers are compared as the
    /// text is searched, with case and punctuation dropped, so `NJ 8255`
    /// after `NJ-8255` is the same number again. The first spelling of a
    /// chosen number stands, in the order it was chosen. The struck-out
    /// numbers are left as they are, and so is every chosen one they name.
    pub fn normalized(mut self) -> Self {
        self.search_words = self.search_words.take().and_then(SearchWords::trimmed);
        let mut kept: Vec<String> = Vec::new();
        self.chosen_catalogs.retain(|value| {
            let key = crate::util::text::squash(value);
            if kept.contains(&key) {
                return false;
            }
            kept.push(key);
            true
        });
        self
    }

    /// Whether these two ask the providers the same thing: the same signals
    /// and the same numbers. What the folder's text is taken to state about
    /// the answers is not part of it — that is read afresh every time the
    /// answers are, which is why changing it needs no run.
    pub fn asks_the_same_as(&self, other: &Self) -> bool {
        self.disc_id_excluded == other.disc_id_excluded
            && self.excluded_barcodes == other.excluded_barcodes
            && self.chosen_catalogs == other.chosen_catalogs
            && self.search_words == other.search_words
    }
}

/// What writing a candidate's choices changed — which is what says whether
/// the answers in hand still answer the question the person is asking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChoiceChange {
    /// The lookups are different now, so what a run in hand found answers a
    /// question nobody is asking any more: the caller starts a run that reads
    /// the new value.
    Lookups,
    /// The same lookups, and only what the folder's text is taken to state
    /// about their answers is different. Nothing is asked again; the next
    /// read of the candidate ranks the stored answers by the new value.
    Ranking,
}

#[cfg(test)]
mod tests {
    use super::{LookupChoices, SearchWords};

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    /// Striking a number out and choosing it are separate choices: a number
    /// in both lists stays in both, whichever way the two lists spell it.
    #[test]
    fn a_struck_out_number_stays_chosen() {
        let normalized = LookupChoices {
            disc_id_excluded: false,
            excluded_barcodes: Vec::new(),
            chosen_catalogs: strings(&["WPCR-80001", "NJ 8255", "COCQ 84487"]),
            search_words: None,
            discounted_catalogs: strings(&["nj-8255"]),
        }
        .normalized();
        assert_eq!(
            normalized.chosen_catalogs,
            strings(&["WPCR-80001", "NJ 8255", "COCQ 84487"])
        );
        assert_eq!(normalized.discounted_catalogs, strings(&["nj-8255"]));
    }

    /// One number is chosen once: the second spelling of it is dropped and
    /// the first stands where it was chosen.
    #[test]
    fn a_number_is_chosen_once_however_it_is_spelled() {
        let normalized = LookupChoices {
            chosen_catalogs: strings(&["NJ-8255", "WPCR-80001", "NJ 8255"]),
            ..LookupChoices::default()
        }
        .normalized();
        assert_eq!(
            normalized.chosen_catalogs,
            strings(&["NJ-8255", "WPCR-80001"])
        );
    }

    /// Typed words keep their letters and lose their edges; words naming no
    /// title are no words, and the draft's title is searched.
    #[test]
    fn search_words_are_trimmed_and_blank_ones_are_none() {
        let typed = LookupChoices {
            search_words: Some(SearchWords {
                album: "  Album Title  ".to_string(),
                artist: " Artist ".to_string(),
            }),
            ..LookupChoices::default()
        }
        .normalized();
        assert_eq!(
            typed.search_words,
            Some(SearchWords {
                album: "Album Title".to_string(),
                artist: "Artist".to_string(),
            })
        );
        let blank = LookupChoices {
            search_words: Some(SearchWords {
                album: "   ".to_string(),
                artist: "Artist".to_string(),
            }),
            ..LookupChoices::default()
        }
        .normalized();
        assert_eq!(blank.search_words, None);
    }

    /// Different words are a different question for the providers.
    #[test]
    fn different_search_words_ask_something_else() {
        let draft = LookupChoices::default();
        let typed = LookupChoices {
            search_words: Some(SearchWords {
                album: "Album Title".to_string(),
                artist: String::new(),
            }),
            ..LookupChoices::default()
        };
        assert!(!draft.asks_the_same_as(&typed));
        assert!(typed.asks_the_same_as(&typed.clone()));
    }
}
