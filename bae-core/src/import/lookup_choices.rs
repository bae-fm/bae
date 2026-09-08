//! What a person has decided one candidate's identification asks about.
//!
//! Extraction reads a folder's disc ID, its barcodes and its catalog numbers;
//! which of them a run actually looks up is a separate question, and the
//! answer is the person's. They uncheck a barcode that belongs to the box set
//! rather than the disc, or pick the one catalog number of thirty that names
//! this pressing — and that decision outlives the run they made it during.
//!
//! So it is stored with the candidate rather than held inside a running
//! reducer: a run reads it at its start, and every later run reads the same
//! value until the person changes it. Changing it is what starts the next run.

/// The signals one candidate's identification leaves out, and the catalog
/// numbers it asks about.
///
/// The default is what a candidate nobody has touched runs with: the disc ID
/// and the barcodes are asked about, and no catalog number is — one number can
/// name thirty releases, so a number is looked up only once someone says it is
/// this disc's.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LookupChoices {
    /// Whether the run leaves the candidate's disc ID out.
    pub disc_id_excluded: bool,
    /// Whether the run leaves the candidate's barcodes out.
    pub barcode_excluded: bool,
    /// The catalog numbers the run looks up, each on its own, in the order
    /// they were chosen — which is the order their lookups are dispatched and
    /// their results laid out.
    pub chosen_catalogs: Vec<String>,
}
