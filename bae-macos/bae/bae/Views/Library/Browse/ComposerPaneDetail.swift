import BaeKit

/// What the composer browse mode's detail pane is showing: nothing, a composer
/// with its page's rows once their first page has answered (optionally with
/// one of its works loaded inline), or a standalone work. Composed from the
/// app-owned projection store, so both selection and delivered detail survive
/// a library-view remount.
enum ComposerPaneDetail {
    case empty
    case composer(
        BridgeComposerDetail,
        rows: ComposerDetailRowList?,
        work: BridgeWorkDetail?
    )
    case work(BridgeWorkDetail)
}
