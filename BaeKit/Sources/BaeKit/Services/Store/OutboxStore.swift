import SwiftUI
import os.log

private let logger = Logger.bae("OutboxStore")

/// One foreground attempt to admit a cloud-upload batch. The store retains the
/// attempt's identity so an overlapping command can fail without clearing a
/// handoff still owned by another command.
public final class CloudUploadCommand: Sendable {
    fileprivate let releaseIds: [String]

    fileprivate init(releaseIds: [String]) {
        self.releaseIds = releaseIds
    }
}

/// Mirror of core's cloud outbox processing snapshot, rendered by the Storage
/// Manager's queue panel and used by every storage row to read its
/// per-release upload progress and rate (no cached `pendingUploads` field on
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
    private var cloudUploadCommands: [String: Set<ObjectIdentifier>] = [:]

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
            switch handoff {
            case .queueing:
                return snapshot.perRelease[releaseId] == nil
            case .awaiting(let revision):
                return snapshot.revision < revision
            }
        }
        cloudUploadCommands = cloudUploadCommands.filter {
            cloudUploadHandoffs[$0.key] == .queueing
        }
    }

    /// Per-release upload progress, or nil if the release has no work in
    /// flight. Storage rows read this to render their badge and to suppress
    /// storage actions while uploads are in flight.
    public func progress(forRelease releaseId: String) -> BridgeUploadProgress?
    {
        snapshot.perRelease[releaseId]?.progress
    }

    /// Whether the release has upload work queued or in flight. Drives the
    /// storage row's "Cancel Upload" affordance and the suppression of other
    /// storage actions while a transfer is mid-flight. Releases with nothing
    /// left to ship are absent from the per-release map, so presence is the
    /// signal.
    public func isTransitioning(forRelease releaseId: String) -> Bool {
        storageUploadObservation(forRelease: releaseId) != nil
    }

    /// Begin the foreground portion of one batch before the bridge can admit
    /// its durable work. Overlapping attempts share the queueing handoff until
    /// every attempt fails or one publishes a durable queue revision.
    public func beginCloudUploads(forReleases releaseIds: [String])
        -> CloudUploadCommand
    {
        let command = CloudUploadCommand(releaseIds: releaseIds)
        let selected = Set(releaseIds)
        guard !releaseIds.isEmpty, selected.count == releaseIds.count else {
            return command
        }
        let commandId = ObjectIdentifier(command)
        for releaseId in releaseIds {
            guard snapshot.perRelease[releaseId] == nil else { continue }
            switch cloudUploadHandoffs[releaseId] {
            case nil:
                cloudUploadHandoffs[releaseId] = .queueing
                cloudUploadCommands[releaseId] = [commandId]
            case .queueing:
                cloudUploadCommands[releaseId, default: []].insert(commandId)
            case .awaiting:
                break
            }
        }
        return command
    }

    /// Finish one foreground command from its exact durable receipt. Releases
    /// in the receipt hand off to its outbox revision; refused releases drop
    /// only this command's queueing ownership. If the retained value already won
    /// the callback race, it remains the authority; if an admitted upload already
    /// finished, no transition remains.
    public func finishCloudUploads(
        for command: CloudUploadCommand,
        receipt: BridgeUploadReceipt?
    ) {
        let admittedReleaseIds: Set<String>
        switch receipt {
        case .some(let receipt):
            admittedReleaseIds = Set(receipt.releaseIds)
            precondition(
                admittedReleaseIds.count == receipt.releaseIds.count
                    && admittedReleaseIds.isSubset(of: command.releaseIds),
                "a cloud-upload receipt must name unique releases from its command"
            )
        case .none:
            admittedReleaseIds = []
        }

        let commandId = ObjectIdentifier(command)
        for releaseId in command.releaseIds {
            if admittedReleaseIds.contains(releaseId) {
                cloudUploadCommands.removeValue(forKey: releaseId)
                guard let receipt else {
                    preconditionFailure(
                        "an admitted release requires a receipt"
                    )
                }
                if snapshot.revision >= receipt.outboxRevision {
                    cloudUploadHandoffs.removeValue(forKey: releaseId)
                }
                else {
                    cloudUploadHandoffs[releaseId] = .awaiting(
                        revision: receipt.outboxRevision
                    )
                }
                continue
            }
            guard var commands = cloudUploadCommands[releaseId] else {
                continue
            }
            commands.remove(commandId)
            if commands.isEmpty {
                cloudUploadCommands.removeValue(forKey: releaseId)
                if cloudUploadHandoffs[releaseId] == .queueing {
                    cloudUploadHandoffs.removeValue(forKey: releaseId)
                }
            }
            else {
                cloudUploadCommands[releaseId] = commands
            }
        }
    }

    /// A Storage surface's full upload state, including the short command-to-
    /// subscription handoff. Once a retained snapshot contains the release it
    /// always wins.
    public func storageUploadObservation(forRelease releaseId: String)
        -> StorageUploadObservation?
    {
        if let releaseProgress = snapshot.perRelease[releaseId] {
            return .active(
                progress: releaseProgress.progress,
                throughputBps: releaseProgress.throughputBps
            )
        }
        return switch cloudUploadHandoffs[releaseId] {
        case .queueing: .queueing
        case .awaiting: .awaiting
        case nil: nil
        }
    }

    /// The idle queue used before the required initial snapshot arrives and by
    /// previews/tests. A failed initial read fails library opening instead of
    /// presenting this value as authoritative queue state.
    public static var emptySnapshot: BridgeOutboxSnapshot {
        BridgeOutboxSnapshot(
            revision: 0,
            uploadGroups: [],
            otherFiles: [],
            perRelease: [:],
            total: BridgeUploadProgress(
                queued: 0,
                uploading: 0,
                bar: nil,
                activity: nil,
            ),
            summaryParts: [],
            pauseState: .running,
            throughputBps: 0,
            etaSeconds: nil,
        )
    }
}

/// A release's cloud transition while it has one. A release with nothing left
/// to ship has none at all, which every surface reads as absence rather than
/// as a value.
public enum UploadObservation: Equatable {
    case awaiting
    case active(BridgeUploadProgress)

    /// The dominant phase alone, for a line that puts the bar between the
    /// phase and its count. It names the release's phase, not how many of
    /// its files are in it.
    public var phaseText: String {
        switch self {
        case .awaiting:
            BridgeUploadActivity.queued.phaseText
        case .active(let progress):
            progress.heldActivity.phaseText
        }
    }

    /// The bytes the counting phase has moved, while one is counting.
    public var progressDetailText: String? {
        switch self {
        case .awaiting:
            return nil
        case .active(let progress):
            return progress.bar?.text
        }
    }
}

public enum StorageUploadObservation: Equatable {
    case queueing
    case awaiting
    case active(progress: BridgeUploadProgress, throughputBps: UInt64)

    /// Cancellation targets work coven has durably admitted; the command
    /// handoff has nothing to cancel yet.
    public var canCancel: Bool {
        switch self {
        case .queueing, .awaiting:
            false
        case .active:
            true
        }
    }

    public var transitionStatusText: String {
        [transitionPhaseText, progressDetailText]
            .compactMap { $0 }
            .joined(separator: " \u{00B7} ")
    }

    /// The transition's phase alone, for a line that puts the bar between
    /// the phase and its count.
    public var transitionPhaseText: String {
        switch self {
        case .queueing:
            return NSLocalizedString(
                bridgeTransferActionKey(action: .upload),
                tableName: "Core",
                bundle: .module,
                comment: ""
            )
        case .awaiting:
            return UploadObservation.awaiting.phaseText
        case .active(let progress, _):
            return UploadObservation.active(progress).phaseText
        }
    }

    /// The bytes the counting phase has moved, while one is counting.
    public var progressDetailText: String? {
        switch self {
        case .queueing, .awaiting:
            return nil
        case .active(let progress, _):
            return UploadObservation.active(progress).progressDetailText
        }
    }

    /// The bar the transition draws: indeterminate while the command hands
    /// off, exact progress through the phase counting bytes, and none for
    /// files that are only waiting: nothing is moving them, and with no
    /// storage connected nothing will until one is.
    public var progressBar: CloudTransitionProgress? {
        switch self {
        case .queueing, .awaiting:
            return .indeterminate
        case .active(let progress, _):
            return progress.bar.map { .determinate($0.fraction) }
        }
    }

    /// The release's current upload rate. A zero reading
    /// stays absent instead of presenting a stalled speed.
    public var throughputText: String? {
        guard case .active(_, let throughputBps) = self,
            throughputBps > 0
        else { return nil }
        return QueueSummary.throughputText(bytesPerSecond: throughputBps)
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
