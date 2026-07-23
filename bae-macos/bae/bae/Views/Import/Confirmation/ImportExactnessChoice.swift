/// The pane's Exact / Metadata-only choice, present only for a source-backed
/// pick (absent for Unknown imports). Bundling the state and its handler lets
/// the confirm view take one optional instead of a `canChoose` flag plus a
/// `?? false` default.
struct ImportExactnessChoice {
    let isMetadataOnly: Bool
    /// `true` selects Exact pressing, `false` selects Metadata only.
    let onSelect: (Bool) -> Void
}
