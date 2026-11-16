import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// The release editor's save, cancel and album pick are local and show no
/// spinner; applying a catalog release can wait on the network, so the editor
/// shows the step the apply's row holds to name what it waits on.
@MainActor
@Suite("Release editor waits")
struct ReleaseEditorWaitTests {
    @Test("a save shows no spinner")
    func savingShowsNoSpinner() async throws {
        let session = heldSession()
        try await hosted(session) { host in
            session.save { _ in }
            try await Wait.until { session.isBusy }
            try await SnapshotTestSupport.settle(host)
            let spinners = SnapshotTestSupport.descendants(of: host)
                .filter { $0 is NSProgressIndicator }
            #expect(spinners.isEmpty, "drew \(spinners)")
            session.cancelTasks()
        }
    }

    @Test("an apply shows its step")
    func applyingFollowsItsSteps() async throws {
        let session = heldSession()
        try await hosted(session) { host in
            #expect(!showsStep(host))

            session.apply(MappingFixtures.link)
            try await Wait.until { session.isBusy }
            try await SnapshotTestSupport.settle(host)

            #expect(showsStep(host))
            session.cancelTasks()
        }
    }

    /// A session whose save and apply never finish, so the editor stays on
    /// its wait.
    private func heldSession() -> ReleaseMetadataEditSession {
        let draft = PreviewData.releaseEditDraft(trackCount: 2)
        let held = ReleaseEditSessionCommands(
            draft: { draft },
            setField: { _, _ in draft },
            setLabels: { _ in draft },
            setBarcodes: { _ in draft },
            setPressingFact: { _ in draft },
            setAlbumArtists: { _ in draft },
            setTrack: { _ in draft },
            apply: { _, _ in try await Task.neverAnswers() },
            setAlbum: { _ in draft },
            save: { try await Task.neverAnswers() }
        )
        let opened = OpenedReleaseEdit(draft: draft, session: held)
        return ReleaseMetadataEditSession(
            releaseId: "release-test",
            opened: opened,
            commands: ReleaseEditCommands(
                open: { _ in opened },
                searchAlbums: { _ in [] },
                catalogApply: { nil }
            )
        )
    }

    private func hosted(
        _ session: ReleaseMetadataEditSession,
        _ body: @MainActor (NSView) async throws -> Void
    ) async throws {
        let size = NSSize(width: 1_000, height: 700)
        try await SnapshotTestSupport.withHostedWindow(
            ReleaseEditorView(
                session: session,
                displayName: "Album Title",
                onViewAlbum: { _ in },
                onSaved: { _ in }
            )
            .frame(width: size.width, height: size.height)
            // No bridge is behind the apply, so its line draws this step.
            .environment(
                \.heldCatalogApplyStep,
                .fetchingCover(catalog: .discogs)
            )
            .environment(PreviewData.artistAssignmentsLibrary())
            .environment(SettingsNavigation())
            .albumDetailPreviewEnvironment()
            .candidateReaderPreviewEnvironment()
            .mainWindowPreviewEnvironment(),
            size: size
        ) { _, host in
            try await SnapshotTestSupport.settle(host)
            try await body(host)
        }
    }

    /// Whether the editor shows an apply's step: the step's bar.
    private func showsStep(_ host: NSView) -> Bool {
        SnapshotTestSupport.descendants(of: host)
            .contains { $0 is ProgressTrackNSView }
    }
}
