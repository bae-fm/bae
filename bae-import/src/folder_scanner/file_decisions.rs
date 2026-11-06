//! The person's decisions about a folder's files, which a scan applies over
//! what it proposes: which audio each track sheet's `FILE` references
//! describe and which disc each sheet's entries become.

use super::*;

/// Which disc of the release one track sheet's entries become.
///
/// Cue filenames are arbitrary — `CD1.cue` may hold disc two — so the mapping
/// cannot read the order off the names. This is the answer, and like a sheet's
/// binding it is the user's to overrule and it survives a restart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SheetDisc {
    /// The sheet's entries are the release's disc `number`, counting from one.
    Disc { number: u32 },
    /// The sheet contributes nothing to the tracklist. Its container is loose
    /// audio again, exactly as an unbound sheet leaves it.
    Ignored,
}

/// Every decision of one kind the user has made for one candidate, keyed by
/// the file's [`ScannedFile::relative_path`].
///
/// A file *absent* from this is not a decision — whatever the scan proposed
/// stands. Undoing a decision is therefore a value of its own (each alias below
/// names the one it uses), never a removed entry: re-guessing after someone
/// said the guess was wrong is the one answer certainly not asked for.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct Edits<V>(BTreeMap<String, V>);

// Derived `Default` would demand `V: Default`, and none of the decisions has a
// default value — an absent entry is what "no decision" means.
impl<V> Default for Edits<V> {
    fn default() -> Self {
        Self(BTreeMap::new())
    }
}

impl<V> Edits<V> {
    /// The user's decision for one file, or `None` when they have made none.
    pub fn get(&self, file_id: &str) -> Option<&V> {
        self.0.get(file_id)
    }

    /// Record one file's decision, replacing any previous one.
    pub fn set(&mut self, file_id: String, decision: V) {
        self.0.insert(file_id, decision);
    }

    /// Every decision, in file-id order — what the store writes as rows.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &V)> {
        self.0.iter().map(|(id, decision)| (id.as_str(), decision))
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Every disc assignment the user has decided for one candidate.
///
/// A sheet *absent* from this is not a decision — it takes its own position
/// among the folder's bound sheets, in `relative_path` order.
pub type SheetDiscEdits = Edits<SheetDisc>;

/// One track sheet's binding as the *user* set it — the second writer of
/// [`SheetBinding`], alongside the scan.
///
/// Stored per candidate and applied over whatever the scan proposed, so the
/// two never fight: the scan proposes on every walk, and this overrides.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum UserSheetBinding {
    /// Bound to the audio at this [`ScannedFile::relative_path`].
    Describes { file_id: String },
    /// The user cleared the binding: the sheet is unbound, and the scan's
    /// proposal is *not* restored. Someone who cleared a binding is saying the
    /// guess was wrong, so re-guessing it is the one answer that is certainly
    /// not what they asked for.
    Cleared,
}

/// Every sheet binding the user has decided for one candidate.
///
/// A sheet *absent* from this is not a decision — it means nobody has touched
/// that sheet and the scan's proposal stands. That is why clearing stores
/// [`UserSheetBinding::Cleared`] rather than removing the entry.
pub type SheetBindingEdits = Edits<Edits<UserSheetBinding>>;

impl SheetBindingEdits {
    pub fn set_reference(
        &mut self,
        sheet_id: String,
        reference: String,
        decision: UserSheetBinding,
    ) {
        self.0.entry(sheet_id).or_default().set(reference, decision);
    }
}

/// Everything the user has settled about one candidate's track sheets: which
/// audio each `FILE` reference describes, and which disc each sheet's entries
/// become.
///
/// One value because they settle together — both are keyed by the same
/// content hash and read by the same scan, and asking for them separately
/// would be two things to keep in step.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CandidateFileEdits {
    pub sheet_bindings: SheetBindingEdits,
    pub sheet_discs: SheetDiscEdits,
    pub revision: u64,
}

impl CandidateFileEdits {
    /// Whether these decisions stand at `revision`: the one a candidate, a
    /// verdict or a write read them at.
    pub fn is_at_revision(&self, revision: u64) -> bool {
        self.revision == revision
    }

    pub fn is_empty(&self) -> bool {
        self.sheet_bindings.is_empty() && self.sheet_discs.is_empty()
    }

    /// These decisions as a release holding the files under `prefix` reads
    /// them: every file named by its path under the prefix, and every disc a
    /// sheet was assigned renumbered by `disc` into the run the release gives
    /// this folder. The revision is not carried: it is the release's own.
    pub(crate) fn under_prefix(&self, prefix: &str, disc: impl Fn(u32) -> u32) -> Self {
        let named = |file_id: &str| format!("{prefix}{file_id}");
        let mut edits = Self::default();
        for (sheet_id, references) in self.sheet_bindings.iter() {
            for (reference, decision) in references.iter() {
                let decision = match decision {
                    UserSheetBinding::Describes { file_id } => {
                        UserSheetBinding::Describes { file_id: named(file_id) }
                    }
                    UserSheetBinding::Cleared => UserSheetBinding::Cleared,
                };
                edits.sheet_bindings.set_reference(
                    named(sheet_id),
                    reference.to_string(),
                    decision,
                );
            }
        }
        for (sheet_id, assigned) in self.sheet_discs.iter() {
            let assigned = match assigned {
                SheetDisc::Disc { number } => SheetDisc::Disc { number: disc(*number) },
                SheetDisc::Ignored => SheetDisc::Ignored,
            };
            edits.sheet_discs.set(named(sheet_id), assigned);
        }
        edits
    }

    /// Every decision in `over` in place of whatever these say about the
    /// same file, taking `over`'s revision.
    pub(crate) fn overlay(&mut self, over: &Self) {
        for (sheet_id, references) in over.sheet_bindings.iter() {
            for (reference, decision) in references.iter() {
                self.sheet_bindings.set_reference(
                    sheet_id.to_string(),
                    reference.to_string(),
                    decision.clone(),
                );
            }
        }
        for (sheet_id, assigned) in over.sheet_discs.iter() {
            self.sheet_discs.set(sheet_id.to_string(), *assigned);
        }
        self.revision = over.revision;
    }
}

/// The file decisions every candidate has stored, keyed by content hash.
///
/// A scan is what computes a candidate's content hash, so the lookup has to
/// happen *inside* the scan: a caller hands in the whole stored set rather than
/// trying to pick one candidate's row without the key that addresses it.
#[derive(Debug, Clone, Default)]
pub struct StoredCandidateEdits(HashMap<String, CandidateFileEdits>);

impl StoredCandidateEdits {
    pub fn new(by_content_hash: HashMap<String, CandidateFileEdits>) -> Self {
        Self(by_content_hash)
    }

    /// Nothing stored: every file and every sheet keeps the scan's proposal.
    pub fn none() -> Self {
        Self::default()
    }

    pub(super) fn for_hash(&self, content_hash: &str) -> Option<&CandidateFileEdits> {
        self.0.get(content_hash)
    }

    pub(crate) fn revision_for_hash(&self, content_hash: &str) -> u64 {
        self.for_hash(content_hash).map_or(0, |edits| edits.revision)
    }

    /// One candidate's decisions, to be added to and written back. Empty when
    /// it has none yet.
    pub fn take(mut self, content_hash: &str) -> CandidateFileEdits {
        self.0.remove(content_hash).unwrap_or_default()
    }
}
