import BaeKit

/// Everything `ImportSearchPane` renders from — the identify/results state and
/// the surrounding flags. The editable form fields (search bindings) and the
/// actions stay separate; this is the read-only display state.
struct ImportSearchState {
    let identifyState: IdentifyState
    let showManualSearch: Bool
    let error: String?
    let searchGroups: [ReleaseGroup]
    /// Release id of the pressing whose confirm pane is open, so its row renders
    /// selected.
    let selectedReleaseId: String?
    let isSearching: Bool
    let hasSearched: Bool
    let isImporting: Bool
    let libraryStatuses: [String: BridgeLibraryStatus]
    let discogsEnabled: Bool
    let signals: Signals?
    /// The interactive signals toolbar — the pre-shaped badge list. Empty until
    /// the first identify transition; the toolbar is hidden until then.
    let signalsToolbar: BridgeSignalsToolbar
}
