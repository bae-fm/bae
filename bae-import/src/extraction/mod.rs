//! Signal extraction, an import stage: one pass over a candidate's files, or
//! a library release's, that reads the
//! [`bae_identify::signals::Signals`] identification looks up and narrows
//! by.

mod analyzer;
pub(crate) mod candidate_text;
mod fast_pass;
mod pool;
pub mod release;
pub mod service;

pub use analyzer::{ArtworkAnalysis, ArtworkAnalyzer, ArtworkUnreadable};
pub use service::{ExtractionService, ExtractionSource, ExtractionWatch, SignalsSnapshot};
