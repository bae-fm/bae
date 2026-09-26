import Foundation
import Observation

/// Slim per-release projection — what list views (storage manager,
/// release pickers) render one row per release. Identity-stable
/// `@Observable` class: in-band mutations like pin toggle, size
/// updates, or media fixes re-render the row without rebuilding the
/// list.
///
/// Composed into [`ReleaseDetail`] — the detail wraps its summary so
/// consumers that hold a detail treat it as a superset. Interning a
/// detail also interns its summary.
///
/// Per-release UPLOAD progress is NOT a field here. The `OutboxStore`
/// snapshot is the single source of truth for the queue; storage rows
/// and storage-action gates read `outboxStore.progress(forRelease:)`.
///
/// Per-release TRANSFER progress (the synchronous pin/unpin/cloud/local
/// transition the user triggers) IS a field here: `transfer`. Core includes the
/// current action in subscribed release values. The two concerns are distinct —
/// uploads are background queue work, a transfer is a foregrounded action with
/// its own determinate bar.

/// An in-flight storage transition for a release.
public struct TransferState: Equatable {
    public let label: String
}

@Observable
public final class ReleaseSummary: Identifiable {
    public let id: String
    public let albumId: String
    /// What the release is made of, each carrier with its count.
    public var media: [BridgeMediaCount]
    /// The same media as core words them: "2×CD", "Vinyl".
    public var mediaTerms: [BridgeFactTerm]
    public var storageState: BridgeReleaseStorageState
    /// Whether coven keeps this release's blobs pinned locally on this device
    /// — the orthogonal coven-cache property, meaningful only when
    /// `storageState` is `.remote`. Kept separate from `storageState` so the UI
    /// never conflates "in the cloud" with "pinned locally".
    public var pinned: Bool
    /// Storage transitions the user can take right now, pre-computed by core
    /// from the release's state and cloud-home presence. The album-detail
    /// "Storage…" sheet and the Storage Manager row context menu render these;
    /// neither re-derives availability.
    public var storageActions: [BridgeReleaseStorageAction]
    public var fileCount: Int64
    public var totalSize: Int64
    /// Reference to this release's own cover image (id + content version), or
    /// `nil` when none is cached. Keyed on the release id so each release
    /// renders its own art; `ImageView` fetches the bytes by id and caches the
    /// decoded image under the version.
    public var cover: BridgeImageRef?
    /// Non-nil while a pin/unpin/cloud/local transition runs. Core includes
    /// the current action in release subscriptions, so later values refresh this
    /// the same way they refresh storage state.
    public var transfer: TransferState?

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
    }

    /// Build a summary from the fat `BridgeRelease` wire type — the summary
    /// half of `LibraryStore.internReleaseDetail(_:)`'s two-slice write.
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
    }

    /// Per-field conditional assignment. Only fields that changed
    /// trigger @Observable re-render.
    public func update(from bridge: BridgeReleaseSummary) {
        if media != bridge.media {
            media = bridge.media
        }
        if mediaTerms != bridge.mediaTerms {
            mediaTerms = bridge.mediaTerms
        }
        if storageState != bridge.storageState {
            storageState = bridge.storageState
        }
        if pinned != bridge.pinned {
            pinned = bridge.pinned
        }
        if storageActions != bridge.storageActions {
            storageActions = bridge.storageActions
        }
        if fileCount != bridge.fileCount {
            fileCount = bridge.fileCount
        }
        if totalSize != bridge.totalSize {
            totalSize = bridge.totalSize
        }
        if cover != bridge.cover {
            cover = bridge.cover
        }
        let transfer = Self.transferState(from: bridge.transferAction)
        if self.transfer != transfer {
            self.transfer = transfer
        }
    }

    public func update(from bridge: BridgeRelease) {
        if media != bridge.facts.media {
            media = bridge.facts.media
        }
        if mediaTerms != bridge.mediaTerms {
            mediaTerms = bridge.mediaTerms
        }
        if storageState != bridge.storageState {
            storageState = bridge.storageState
        }
        if pinned != bridge.pinned {
            pinned = bridge.pinned
        }
        if storageActions != bridge.storageActions {
            storageActions = bridge.storageActions
        }
        if fileCount != bridge.fileCount {
            fileCount = bridge.fileCount
        }
        if totalSize != bridge.totalSize {
            totalSize = bridge.totalSize
        }
        if cover != bridge.cover {
            cover = bridge.cover
        }
        let transfer = Self.transferState(from: bridge.transferAction)
        if self.transfer != transfer {
            self.transfer = transfer
        }
    }

    private static func transferState(
        from action: BridgeReleaseStorageAction?
    ) -> TransferState? {
        action.map {
            TransferState(
                label: NSLocalizedString(
                    bridgeTransferActionKey(action: $0),
                    tableName: "Core",
                    bundle: .module,
                    comment: ""
                )
            )
        }
    }
}
