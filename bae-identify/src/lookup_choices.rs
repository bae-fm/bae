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
    /// struck out: such a number is not in effect — no provider is asked
    /// about it, picked or not, and a result carrying it agrees with the text
    /// about nothing, however plainly the text prints it.
    ///
    /// A set, each value once: nothing dispatches on their order, and a value
    /// is struck out or it is not. A number can be picked and struck out at
    /// once; counting it again puts the pick back in effect.
    pub discounted_catalogs: Vec<String>,
}

/// One change a person makes to what a candidate's identification asks about
/// or counts: each control sends the change it is, and core applies it to the
/// choices it holds, so two quick changes both land.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LookupChoiceEdit {
    /// Ask about the disc ID again, or leave it out.
    ToggleDiscId,
    /// Leave one barcode out, or ask about it again.
    ToggleBarcode { code: String },
    /// Look one catalog number up, or stop looking it up.
    ToggleCatalog { number: String },
    /// Search by these words, or by the draft's own title when both are blank.
    SearchBy { album: String, artist: String },
    /// Strike one catalog number out of what the folder is taken to state, or
    /// count it again.
    ToggleDiscounted { number: String },
}

impl SearchWords {
    /// These words with their edges trimmed, or `None` for words that name
    /// no title — which is no words at all, and the draft's title stands.
    pub fn trimmed(self) -> Option<Self> {
        let album = self.album.trim();
        (!album.is_empty())
            .then(|| Self { album: album.to_string(), artist: self.artist.trim().to_string() })
    }
}

impl LookupChoices {
    /// These choices with `edit` made, as stored: normalized.
    pub fn edited(mut self, edit: LookupChoiceEdit) -> Self {
        fn toggled_in_set(values: &mut Vec<String>, value: String) {
            match values.iter().position(|held| *held == value) {
                Some(index) => {
                    values.remove(index);
                }
                None => values.push(value),
            }
            values.sort();
        }
        match edit {
            LookupChoiceEdit::ToggleDiscId => self.disc_id_excluded = !self.disc_id_excluded,
            LookupChoiceEdit::ToggleBarcode { code } => {
                toggled_in_set(&mut self.excluded_barcodes, code)
            }
            // The chosen numbers are looked up in the order they were chosen,
            // so they stay a list rather than a set.
            LookupChoiceEdit::ToggleCatalog { number } => {
                match self.chosen_catalogs.iter().position(|held| *held == number) {
                    Some(index) => {
                        self.chosen_catalogs.remove(index);
                    }
                    None => self.chosen_catalogs.push(number),
                }
            }
            LookupChoiceEdit::SearchBy { album, artist } => {
                self.search_words = Some(SearchWords { album, artist });
            }
            LookupChoiceEdit::ToggleDiscounted { number } => {
                toggled_in_set(&mut self.discounted_catalogs, number)
            }
        }
        self.normalized()
    }

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
            let key = bae_types::text_match::squash(value);
            if kept.contains(&key) {
                return false;
            }
            kept.push(key);
            true
        });
        self
    }

    /// Whether these two ask the providers the same thing: the same signals,
    /// the same numbers picked and struck out, and the same words. A struck
    /// number is searched by nobody, so striking one changes what is asked.
    pub fn asks_the_same_as(&self, other: &Self) -> bool {
        self.disc_id_excluded == other.disc_id_excluded
            && self.excluded_barcodes == other.excluded_barcodes
            && self.chosen_catalogs == other.chosen_catalogs
            && self.search_words == other.search_words
            && self.discounted_catalogs == other.discounted_catalogs
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
    /// Nothing the run asks changed — words that come to the draft's own
    /// title again. Nothing is asked again.
    Ranking,
}

#[cfg(test)]
#[path = "lookup_choices_tests.rs"]
mod tests;
