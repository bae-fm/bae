import Foundation

/// Slim per-release projection — what list views (storage manager,
/// release pickers) render one row per release. Built from the row a query
/// returned, either the slim `BridgeReleaseSummary` a list reads or the fat
/// `BridgeRelease` a detail reads, so both draw through one shape.
///
/// Composed into [`ReleaseDetail`] — the detail wraps its summary so
/// consumers that hold a detail treat it as a superset.
///
/// What of the release waits in the upload queue is `upload`, read with the
/// row; how fast its files go is its transfer in `OutboxStore`.
///
/// Per-release TRANSFER progress (the synchronous pin/unpin/cloud/local
/// transition the user triggers) IS a field here: `transfer`. Core includes the
/// current action in subscribed release values. The two concerns are distinct —
/// uploads are background queue work, a transfer is a foregrounded action with
/// its own determinate bar.

/// An in-flight storage transition for a release.
public struct TransferState: Equatable, Sendable {
    public let label: String
}

public struct ReleaseSummary: Identifiable, Sendable {
    public let id: String
    public let albumId: String
    /// What the release is made of, each carrier with its count.
    public let media: [BridgeMediaCount]
    /// The same media as core words them: "2×CD", "Vinyl".
    public let mediaTerms: [BridgeFactTerm]
    public let storageState: BridgeReleaseStorageState
    /// Whether coven keeps this release's files pinned locally on this device
    /// — the orthogonal coven-cache property, meaningful only when
    /// `storageState` is `.remote`. Kept separate from `storageState` so the UI
    /// never conflates "in the cloud" with "pinned locally".
    public let pinned: Bool
    /// Storage transitions the user can take right now, pre-computed by core
    /// from the release's state and cloud-home presence. The album-detail
    /// "Storage…" sheet and the Storage Manager row context menu render these;
    /// neither re-derives availability.
    public let storageActions: [BridgeReleaseStorageAction]
    public let fileCount: Int64
    public let totalSize: Int64
    /// Reference to this release's own cover image (id + content version), or
    /// `nil` when none is cached. Keyed on the release id so each release
    /// renders its own art; `ImageView` fetches the bytes by id and caches the
    /// decoded image under the version.
    public let cover: BridgeImageRef?
    /// Non-nil while a pin/unpin/cloud/local transition runs. Core includes
    /// the current action in release subscriptions, so later values carry it
    /// the same way they carry storage state.
    public let transfer: TransferState?
    /// What of the release waits in this device's upload queue; `nil` when
    /// nothing does.
    public let upload: BridgeReleaseUpload?
    /// Whether an upload or a transfer is under way: the release then offers
    /// only cancelling, and `storageActions` is empty.
    public let transitioning: Bool

    /// The media in the current locale's words: "2×CD", "Vinyl".
    public var mediaText: String {
        PressingText.line(mediaTerms)
    }

    /// Total release size formatted for the current locale, e.g. "350 MB".
    /// bae-core emits the raw byte count; the UI formats it.
    public var totalSizeText: String {
        totalSize.formatted(.byteCount(style: .file))
    }

    public init(from bridge: BridgeReleaseSummary) {
        id = bridge.id
        albumId = bridge.albumId
        media = bridge.media
        mediaTerms = bridge.mediaTerms
        storageState = bridge.storageState
        pinned = bridge.pinned
        storageActions = bridge.storageActions
        fileCount = bridge.fileCount
        totalSize = bridge.totalSize
        cover = bridge.cover
        transfer = Self.transferState(from: bridge.transferAction)
        upload = bridge.upload
        transitioning = bridge.transitioning
    }

    /// Build a summary from the fat `BridgeRelease` wire type — the summary
    /// half of a `ReleaseDetail`.
    public init(from bridge: BridgeRelease) {
        id = bridge.id
        albumId = bridge.albumId
        media = bridge.facts.media
        mediaTerms = bridge.mediaTerms
        storageState = bridge.storageState
        pinned = bridge.pinned
        storageActions = bridge.storageActions
        fileCount = bridge.fileCount
        totalSize = bridge.totalSize
        cover = bridge.cover
        transfer = Self.transferState(from: bridge.transferAction)
        upload = bridge.upload
        transitioning = bridge.transitioning
    }

    private static func transferState(
        from action: BridgeReleaseStorageAction?
    ) -> TransferState? {
        action.map { TransferState(label: $0.transferLabel) }
    }
}
