import SwiftUI
import os.log

private let logger = Logger.bae("OutboxStore")

/// Mirror of core's cloud outbox processing snapshot, rendered by the Storage
/// Manager's queue panel and used by every storage row to read its
/// per-release upload count (no cached `pendingUploads` field on
/// `ReleaseSummary`). The retained outbox subscription lands the whole
/// `BridgeOutboxSnapshot`; views read it at the leaf. The snapshot is swapped
/// wholesale (no per-item interning) because core exposes it in full.
@Observable
public class OutboxStore {
    public var snapshot: BridgeOutboxSnapshot
    /// The foreground command before the retained outbox snapshot owns it.
    /// This closes the callback-order gap between the command result and the
    /// outbox subscription without copying durable queue state into Swift.
    private var cloudUploadHandoffs: [String: CloudUploadHandoff] = [:]

    public init(snapshot: BridgeOutboxSnapshot) {
        self.snapshot = snapshot
    }

    public func applySnapshot(_ snapshot: BridgeOutboxSnapshot) {
        guard snapshot.revision >= self.snapshot.revision else {
            logger.debug(
                "dropping outbox snapshot at revision \(snapshot.revision); revision \(self.snapshot.revision) is already applied"
            )
            return
        }
        self.snapshot = snapshot
        cloudUploadHandoffs = cloudUploadHandoffs.filter {
            releaseId,
            handoff in
            if snapshot.perRelease[releaseId] != nil {
                return false
            }
            switch handoff {
            case .queueing:
                return true
            case .awaiting(let revision):
                return snapshot.revision < revision
            }
        }
    }

    /// Per-release upload progress, or nil if the release has no work in
    /// flight. Storage rows read this to render their badge and to suppress
    /// storage actions while uploads are in flight.
    public func progress(forRelease releaseId: String) -> BridgeUploadProgress?
    {
        snapshot.perRelease[releaseId]
    }

    /// Whether the release has upload work queued or in flight. Drives the
    /// storage row's "Cancel Upload" affordance and the suppression of other
    /// storage actions while a transfer is mid-flight. Releases with nothing
    /// left to ship are absent from the per-release map, so presence is the
    /// signal.
    public func isTransitioning(forRelease releaseId: String) -> Bool {
        storageUploadObservation(forRelease: releaseId) != nil
    }

    /// Begin the foreground portion of a Storage action before the bridge call
    /// can enqueue its durable work.
    public func beginCloudUpload(forRelease releaseId: String) {
        precondition(
            snapshot.perRelease[releaseId] == nil
                && cloudUploadHandoffs[releaseId] == nil,
            "release already has a cloud upload transition"
        )
        cloudUploadHandoffs[releaseId] = .queueing
    }

    /// Hand a successful Storage command to the retained outbox revision it
    /// published. If that value already won the callback race, it remains the
    /// authority; if the whole upload already finished, no transition remains.
    public func cloudUploadQueued(
        forRelease releaseId: String,
        atRevision revision: UInt64
    ) {
        precondition(
            cloudUploadHandoffs[releaseId] == .queueing
                || snapshot.perRelease[releaseId] != nil,
            "cloud upload receipt arrived without its foreground command"
        )
        if snapshot.perRelease[releaseId] != nil
            || snapshot.revision >= revision
        {
            cloudUploadHandoffs.removeValue(forKey: releaseId)
        }
        else {
            cloudUploadHandoffs[releaseId] = .awaiting(revision: revision)
        }
    }

    /// End a foreground command that failed before durable enqueue.
    public func cloudUploadFailed(forRelease releaseId: String) {
        precondition(
            cloudUploadHandoffs.removeValue(forKey: releaseId) != nil,
            "cloud upload failed without its foreground command"
        )
    }

    /// A Storage surface's full upload state, including the short command-to-
    /// subscription handoff. Once a retained snapshot contains the release it
    /// always wins.
    public func storageUploadObservation(forRelease releaseId: String)
        -> StorageUploadObservation?
    {
        if let progress = snapshot.perRelease[releaseId] {
            return .active(progress)
        }
        return switch cloudUploadHandoffs[releaseId] {
        case .queueing: .queueing
        case .awaiting: .awaiting
        case nil: nil
        }
    }

    /// A cloud import's relationship to the durable outbox. Its import result
    /// carries the revision that represented the durable enqueue. A lower local
    /// revision is still awaiting that value; the same-or-newer revision either
    /// contains active progress or proves the transition already completed.
    public func uploadObservation(
        forRelease releaseId: String,
        queuedAtRevision: UInt64
    )
        -> UploadObservation
    {
        if let progress = snapshot.perRelease[releaseId] {
            return .active(progress)
        }
        return snapshot.revision >= queuedAtRevision ? .finished : .awaiting
    }

    /// A restored import has no in-process enqueue revision. Its release ID
    /// rejoins the durable queue directly: presence means active work, while
    /// absence means the imported row is resting.
    public func persistedUploadObservation(forRelease releaseId: String)
        -> UploadObservation?
    {
        snapshot.perRelease[releaseId].map(UploadObservation.active)
    }

    /// Whether any cloud writes are still queued or in flight — uploads or
    /// deletes that haven't reached the cloud home. Drives the extra
    /// data-loss warning on the remove-library confirmation.
    public var hasPendingCloudWork: Bool {
        !snapshot.uploadGroups.isEmpty || snapshot.pendingDeletes > 0
    }

    /// The idle queue used before the required initial snapshot arrives and by
    /// previews/tests. A failed initial read fails library opening instead of
    /// presenting this value as authoritative queue state.
    public static var emptySnapshot: BridgeOutboxSnapshot {
        BridgeOutboxSnapshot(
            revision: 0,
            uploadGroups: [],
            deletes: [],
            perRelease: [:],
            total: BridgeUploadProgress(
                queued: 0,
                preparing: 0,
                prepared: 0,
                uploading: 0,
                retrying: 0,
                uploaded: 0,
                publishing: 0,
                cancelling: 0,
                bar: nil,
                activity: nil,
                canCancel: false,
            ),
            pendingDeletes: 0,
            summaryParts: [],
            pauseState: .running,
            throughputBps: 0,
            etaSeconds: nil,
        )
    }
}

public enum UploadObservation: Equatable {
    case awaiting
    case active(BridgeUploadProgress)
    case finished

    /// The retained queue state rendered by Import after its local commit has
    /// finished. Active work names the dominant phase and, while a phase is
    /// counting bytes, that phase's own numerator and denominator.
    public var statusText: String? {
        switch self {
        case .awaiting:
            return QueueSummary.countLabel("core.queue.queued", 1)
        case .active(let progress):
            guard let phase = progress.activityText else {
                preconditionFailure(
                    "an active cloud upload has no projected activity"
                )
            }
            return [phase, progress.bar?.text]
                .compactMap { $0 }
                .joined(separator: " \u{00B7} ")
        case .finished:
            return nil
        }
    }

    /// Status text for every non-terminal observation. Storage surfaces never
    /// receive `.finished`; calling this for it is a broken state transition.
    public var transitionStatusText: String {
        guard let statusText else {
            preconditionFailure(
                "a terminal cloud upload has no transition text"
            )
        }
        return statusText
    }

    /// Whether the row has no cloud transition, has work with no phase counting
    /// bytes for it yet, or has exact progress through the phase it is in.
    public var progressBar: CloudTransitionProgress? {
        switch self {
        case .awaiting:
            return .indeterminate
        case .active(let progress):
            return progress.bar.map { .determinate($0.fraction) }
                ?? .indeterminate
        case .finished:
            return nil
        }
    }
}

public enum StorageUploadObservation: Equatable {
    case queueing
    case awaiting
    case active(BridgeUploadProgress)

    /// Cancellation only targets work coven has durably admitted. The command
    /// handoff has nothing to cancel yet. Core owns whether an admitted phase
    /// can still be unwound.
    public var canCancel: Bool {
        switch self {
        case .queueing, .awaiting:
            false
        case .active(let progress):
            progress.canCancel
        }
    }

    public var transitionStatusText: String {
        switch self {
        case .queueing:
            return NSLocalizedString(
                bridgeTransferActionKey(action: .makeRemote),
                tableName: "Core",
                bundle: .main,
                comment: ""
            )
        case .awaiting:
            return UploadObservation.awaiting.transitionStatusText
        case .active(let progress):
            return UploadObservation.active(progress).transitionStatusText
        }
    }

    public var progressBar: CloudTransitionProgress {
        switch self {
        case .queueing, .awaiting:
            return .indeterminate
        case .active(let progress):
            return progress.bar.map { .determinate($0.fraction) }
                ?? .indeterminate
        }
    }
}

public enum CloudTransitionProgress: Equatable {
    case indeterminate
    case determinate(Double)

    public var fraction: Double? {
        switch self {
        case .indeterminate: nil
        case .determinate(let fraction): fraction
        }
    }
}

private enum CloudUploadHandoff: Equatable {
    case queueing
    case awaiting(revision: UInt64)
}
