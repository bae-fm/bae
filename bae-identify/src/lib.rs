//! The identify pipeline: matches a candidate's [`signals::Signals`]
//! against external metadata. A pure reducer (`state::step`) asks every key
//! in effect — disc ID, ISRCs, barcodes, the catalog numbers picked or
//! confirmed, the album title when the folder's own keys name nothing, and
//! each offered row's pressing on the catalog it has no record of — round
//! after round until nothing new comes into effect, and `combine` ranks what
//! they return. Import's identify service feeds it extraction snapshots and
//! lookup results and publishes each state as an
//! `ImportEvent::IdentifyStateChanged`. [`view`] shapes a state for surfaces,
//! [`verdict::TerminalVerdict`] is what a terminal state stores, and
//! [`verdict_summary`] reads a stored verdict back as what the queue needs.
//!
//! Identification is part of the import pipeline, which only the desktop
//! builds carry: a mobile build compiles this crate empty.
#![cfg(desktop)]

pub mod agreements;
pub mod combine;
pub mod documents;
pub mod fit;
pub(crate) mod look_alike;
pub mod lookup_choices;
pub(crate) mod medium;
mod not_asked;
pub(crate) mod notes;
pub(crate) mod pressing_evidence;
pub mod release_group;
pub mod row_facts;
pub mod shared_album;
pub mod signals;
pub mod state;
pub mod verdict;
pub mod verdict_summary;
pub mod view;

pub use agreements::{Agreements, CandidateText};
pub use combine::{Findings, LookupAnswers, NamedNote, NarrowedOut};
pub use fit::{unattended_pick, Declined, TracklistFit, UnattendedPick};
pub use lookup_choices::{ChoiceChange, LookupChoiceEdit, LookupChoices, SearchWords};
pub use medium::MediumConflict;
pub use not_asked::NotAskedReason;
pub use release_group::PressingLink;
pub use shared_album::{AlbumLink, SharedAlbum};
pub use state::{
    BarcodeProgress, CatalogProgress, DiscIdAnswer, DiscLookup, DiscidProgress, IdentifyEvent,
    IdentifyState, IsrcProgress, LookupOutcome, LookupResults, LookupState, ProviderLookup,
    SearchProgress, TitleSearch, ValueLookup,
};
pub use verdict::{IdentifyFailure, TerminalVerdict};
pub use verdict_summary::{FolderCheck, VerdictKind, VerdictSummary};
pub use view::{
    BarcodeStepView, CatalogCandidateView, CatalogStepView, DiscIdRow, DiscIdStepView,
    IdentifyRunView, IdentifyStateView, IsrcStepView, JudgedFindings, LookupView, ProviderCell,
    PublishedRun, RowAgreements, RunOutcome, SearchStepView, SignalValueRow,
};
