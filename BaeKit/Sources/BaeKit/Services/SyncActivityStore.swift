import Foundation
import Observation
import os.log

private let logger = Logger.bae("SyncActivityStore")

/// Everything sync is doing on this device, as core reports it: the title
/// bar's indicator, the list it opens, and the quiet lines on albums whose
/// files are still arriving from another device all read it here.
@MainActor
@Observable
public final class SyncActivityStore {
    public private(set) var activity: BridgeSyncActivity?
    /// The last failure to read the activity, or to act from its list.
    public private(set) var error: DisplayError?
    private let cancelArtworkAction: @Sendable () async throws -> Void
    private let retryNowAction: @Sendable () -> Void

    public init(
        activity: BridgeSyncActivity? = nil,
        cancelArtwork: @escaping @Sendable () async throws -> Void,
        retryNow: @escaping @Sendable () -> Void
    ) {
        self.activity = activity
        cancelArtworkAction = cancelArtwork
        retryNowAction = retryNow
    }

    public func apply(_ activity: BridgeSyncActivity) {
        self.activity = activity
        error = nil
    }

    public func fail(_ error: any Error) {
        self.error = DisplayError(error)
    }

    /// The indicator shows while bytes move or something needs the person,
    /// and is absent otherwise.
    public var visible: Bool {
        guard let activity else { return error != nil }
        return activity.moving || activity.needsAttention || error != nil
    }

    /// The device an album is still arriving from, if any of its files is.
    public func arrivingFrom(albumId: String) -> String? {
        guard case .ready(_, let albums, _) = activity?.arrivals else {
            return nil
        }
        return albums[albumId]
    }

    /// Each album still arriving, by id, with the device it arrives from.
    public var arrivingAlbums: [String: String] {
        guard case .ready(_, let albums, _) = activity?.arrivals else {
            return [:]
        }
        return albums
    }

    /// What of a release is still arriving, if anything is.
    public func arrival(releaseId: String) -> BridgeReleaseArrival? {
        guard case .ready(_, _, let releases) = activity?.arrivals else {
            return nil
        }
        return releases[releaseId]
    }

    /// Stop the artwork downloads; a failure shows in the list.
    public func cancelArtwork() async {
        do { try await cancelArtworkAction() }
        catch {
            logger.error("Cancelling artwork downloads failed: \(error)")
            fail(error)
        }
    }

    /// The person's retry: every ordinary wait ends and sync runs now.
    public func retryNow() {
        retryNowAction()
    }
}

#if DEBUG
    extension SyncActivityStore {
        /// A library mid-arrival for previews: a few files uploading, the
        /// artwork downloading, two albums still arriving from a Mac, and a
        /// record that needs signing in again.
        public static func preview(
            needsAttention: Bool = true
        ) -> SyncActivityStore {
            SyncActivityStore(
                activity: .preview(needsAttention: needsAttention),
                cancelArtwork: {},
                retryNow: {}
            )
        }
    }

    extension BridgeSyncActivity {
        /// The activity `SyncActivityStore.preview` shows; the album and
        /// release ids are the preview library's.
        public static func preview(needsAttention: Bool) -> BridgeSyncActivity {
            BridgeSyncActivity(
                moving: true,
                needsAttention: needsAttention,
                failure: nil,
                attention: .ready(
                    records: needsAttention ? [previewSignIn] : []
                ),
                syncing: true,
                uploads: BridgeUploadsInFlight(
                    summaryParts: [
                        BridgeCountLabel(key: "core.queue.uploading", count: 2),
                        BridgeCountLabel(key: "core.queue.queued", count: 10),
                    ],
                    bytesDone: 3_200_000,
                    bytesTotal: 8_400_000
                ),
                artwork: BridgeEagerCacheFillProgress(
                    filesCompleted: 4,
                    filesTotal: 26,
                    bytesCached: 169_000,
                    bytesTotal: 1_720_000
                ),
                downloads: nil,
                arrivals: previewArrivals
            )
        }

        /// Dropbox asking to be signed in to again.
        private static let previewSignIn = BridgePendingRecord(
            subject: BridgeMessage(
                key: "core.pending.subject.connection",
                args: []
            ),
            reason: BridgePendingReason(
                key: "core.pending.reason.storage",
                detail: nil,
                causes: []
            ),
            action: .signIn(provider: .dropbox),
            actionKey: "core.pending.action.sign_in"
        )

        /// Two albums still arriving from a Mac, the first with its cover
        /// and two tracks, and three other pictures.
        private static let previewArrivals = BridgeArrivals.ready(
            devices: [
                BridgeDeviceArrivals(
                    deviceName: "Studio Mac",
                    releases: [
                        BridgeArrivingRelease(
                            releaseId: "r-1",
                            albumId: "a-1",
                            albumTitle: "Album Title One"
                        ),
                        BridgeArrivingRelease(
                            releaseId: "r-2",
                            albumId: "a-2",
                            albumTitle: "Album Title Two"
                        ),
                    ],
                    otherFiles: 3
                )
            ],
            albums: ["a-1": "Studio Mac", "a-2": "Studio Mac"],
            releases: [
                "r-1": BridgeReleaseArrival(
                    deviceName: "Studio Mac",
                    cover: true,
                    trackIds: ["t-3", "t-4"]
                )
            ]
        )
    }
#endif
