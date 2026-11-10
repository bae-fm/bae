import AppKit
import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The error notice's looks and what its Details opens, each beside its
    /// canvas board.
    static let noticeScenes: [ShotScene] = [
        // The import confirm form's banners.
        ShotScene(
            id: "import-confirmation-banners",
            size: CGSize(width: 480, height: 300)
        ) {
            ShotStage(ImportConfirmationBanners.preview)
        },
        ShotScene(
            id: "notice-details-alert",
            size: CGSize(width: 340, height: 320)
        ) {
            ShotStage(
                NSAlertShot {
                    ErrorDetailsAlert.makeAlert(
                        ErrorDetails(
                            title: "Couldn’t load sign-in settings",
                            advice:
                                "This copy of bae can’t read its Dropbox and OneDrive sign-in settings, so it can’t sign in to them. Reinstall bae.",
                            error: DisplayError(
                                line: "The sign-in settings file is missing.",
                                detail: "OAuthConfig: missing client id"
                            )
                        )
                    )
                }
            )
        },
        // A pin that failed, in the Storage Manager's inspector.
        ShotScene(
            id: "notice-download-failed",
            size: CGSize(width: 1_040, height: 654)
        ) {
            .storageManager(
                StorageManagerPreviewScene(
                    selectedReleaseId: "rel-row-3",
                    inspectorPresented: true,
                    downloads: [PreviewData.downloadOps[2]],
                    outputs: [],
                    uploads: []
                )
            )
        },
        // The queue's read of what's playing from failed past its first rows.
        ShotScene(
            id: "notice-queue-tail",
            size: CGSize(width: 420, height: 900)
        ) {
            .queueTail(
                QueueViewPreviewScene.fixture(
                    for: .populated,
                    upcomingRequestFails: true
                )
            )
        },
        // A view that failed whole, in the space a library's grid has.
        ShotScene(
            id: "notice-failed-view",
            size: CGSize(width: 900, height: 520)
        ) {
            ShotStage(
                ErrorNotice(
                    error: DisplayError(
                        line: "Couldn’t read your library.",
                        detail:
                            "SqliteFailure(DatabaseCorrupt)\ndatabase disk image is malformed"
                    ),
                    look: .centered,
                    action: ErrorAction {}
                )
                .background(Theme.background)
            )
        },
        // A release's storage band, its pin failed and its make-local failed.
        ShotScene(
            id: "notice-storage-band-failed",
            size: CGSize(width: 560, height: 260)
        ) {
            ShotStage(StorageBandFailureShot())
        },
        ShotScene(
            id: "notice-looks",
            size: CGSize(width: 480, height: 330)
        ) {
            ShotStage(NoticeLooksShot())
        },
    ]
}

/// A notice with an error behind it, filled with its Details and Retry, then
/// titled in its own words, then quiet.
private struct NoticeLooksShot: View {
    private let error = DisplayError(
        line: "Couldn’t reach your storage.",
        detail: "dns error: failed to lookup address information"
    )

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.group) {
            ErrorNotice(error: error, action: ErrorAction {})
            ErrorNotice(
                title: "bae couldn’t open your library",
                advice: "Check your connection, then quit and reopen bae.",
                error: error
            )
            ErrorNotice(
                title: "Couldn't pin",
                advice: nil,
                error: error,
                look: .quiet,
                action: ErrorAction {}
            )
        }
        .padding(ThemeSpace.section)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        .background(Theme.background)
    }
}

extension ShotStage {
    /// The queue pane over `fixture`, ready once its read of the tail has
    /// failed and the pane has the failure to show.
    fileprivate static func queueTail(_ fixture: QueuePreviewFixture)
        -> ShotStage
    {
        let images = ImageStore.stub()
        return ShotStage(
            QueueViewPreviewScene(width: 420, height: 900)
                .environment(fixture.store)
                .environment(fixture.queue)
                .environment(images),
            images: images
        ) { fixture.store.upcomingFailure != nil }
    }
}

/// The storage band of a release whose download failed.
private struct StorageBandFailureShot: View {
    var body: some View {
        var release = PreviewData.storageRelease(
            storageState: .onOneDevice,
            storageActions: [.upload],
            playability: .playable
        )
        release.downloadStatus = .failed(
            error: "The network connection was lost."
        )
        release.makeLocal = .failed(error: "The disk is full.")
        return StorageStatusBand(
            release: release,
            onAction: { _ in },
            onCancelPin: {},
            onExport: {},
            onSaveAs: {}
        )
        .frame(width: 560)
        .background(Theme.background)
        .environment(OutboxStore(uploadsPaused: false))
        .environment(PreviewData.downloadStore())
    }
}
