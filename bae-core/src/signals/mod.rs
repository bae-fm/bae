//! Signal extraction: one pass over a candidate's files that reads the
//! [`crate::identify::signals::Signals`] identification looks up and narrows
//! by.

mod analyzer;
pub(crate) mod candidate_text;
mod fast_pass;
mod pool;
pub(crate) mod release;
pub mod service;

pub use analyzer::{ArtworkAnalysis, ArtworkAnalyzer};
pub use service::{
    ExtractionService, ExtractionServiceHandle, ExtractionSource, ExtractionWatch, SignalsSnapshot,
};
