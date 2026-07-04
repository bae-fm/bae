import Foundation
import Observation

/// Slim per-release projection — what list views (storage manager,
/// release pickers) render one row per release. Identity-stable
/// `@Observable` class: in-band mutations like pin toggle, size
/// updates, or format fixes re-render the row without rebuilding the
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
/// Per-release TRANSFER progress (the synchronous pin/unpin/manage/unmanage
/// transition the user triggers) IS a field here: `transfer`. Core pushes
/// `ReleaseTransferProgress` events while a transition runs and a terminal
/// `ReleaseTransferEnded` to clear it. The two concerns are distinct — uploads
/// are background queue work, a transfer is a foregrounded action with its own
/// determinate bar.

/// An in-flight storage transition for a release.
struct TransferState: Equatable {
    let label: String
}

@Observable
final class ReleaseSummary: Identifiable {
    let id: String
    let albumId: String
    var format: String?
    var storageState: BridgeReleaseStorageState
    /// Whether coven keeps this release's blobs pinned (offline) on this device
    /// — the orthogonal coven-cache property, meaningful only when
    /// `storageState` is `.managed`. Kept separate from `storageState` so the UI
    /// never conflates "in the cloud" with "kept offline".
    var pinned: Bool
    /// Storage transitions the user can take right now, pre-computed by core
    /// from the release's state and cloud-home presence. The album-detail
    /// "Storage…" sheet and the Storage Manager row context menu render these;
    /// neither re-derives availability.
    var storageActions: [BridgeReleaseStorageAction]
    var fileCount: Int64
    var totalSize: Int64
    /// Reference to this release's own cover image (id + content version), or
    /// `nil` when none is cached. Keyed on the release id so each release
    /// renders its own art; `ImageView` fetches the bytes by id and caches the
    /// decoded image under the version.
    var cover: BridgeImageRef?
    /// Non-nil while a pin/unpin/manage/unmanage transition runs. Set from
    /// `ReleaseTransferProgress`, cleared on `ReleaseTransferEnded`. Not part of
    /// the wire payload — driven entirely by the transfer event stream, so the
    /// wire `update(from:)` paths leave it untouched (a `ReleaseUpdated` landing
    /// mid-transfer must not clear the in-flight indicator).
    var transfer: TransferState?

    /// Total release size formatted for the current locale, e.g. "350 MB".
    /// bae-core emits the raw byte count; the UI formats it.
    var totalSizeText: String {
        totalSize.formatted(.byteCount(style: .file))
    }

    init(from bridge: BridgeReleaseSummary) {
        id = bridge.id
        albumId = bridge.albumId
        format = bridge.format
        storageState = bridge.storageState
        pinned = bridge.pinned
        storageActions = bridge.storageActions
        fileCount = bridge.fileCount
        totalSize = bridge.totalSize
        cover = bridge.cover
    }

    /// Build a summary from the fat `BridgeRelease` wire type. Used by
    /// reducers that receive a full album payload and need to populate
    /// both the summary slice and the detail slice from one event.
    init(from bridge: BridgeRelease) {
        id = bridge.id
        albumId = bridge.albumId
        format = bridge.format
        storageState = bridge.storageState
        pinned = bridge.pinned
        storageActions = bridge.storageActions
        fileCount = bridge.fileCount
        totalSize = bridge.totalSize
        cover = bridge.cover
    }

    /// Per-field conditional assignment. Only fields that changed
    /// trigger @Observable re-render.
    func update(from bridge: BridgeReleaseSummary) {
        if format != bridge.format {
            format = bridge.format
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
    }

    func update(from bridge: BridgeRelease) {
        if format != bridge.format {
            format = bridge.format
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
    }
}
