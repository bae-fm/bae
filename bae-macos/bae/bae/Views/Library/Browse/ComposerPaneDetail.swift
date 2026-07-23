import BaeKit

/// What the composer browse mode's detail pane is showing: nothing, a composer
/// (optionally with one of its works loaded inline), or a standalone work.
/// View-local to `LibraryView` — a remount re-fetches it cheaply rather than
/// keeping it warm, while the selection that drives it persists on the session.
enum ComposerPaneDetail {
    case empty
    case composer(BridgeComposerDetail, work: BridgeWorkDetail?)
    case work(BridgeWorkDetail)
}
