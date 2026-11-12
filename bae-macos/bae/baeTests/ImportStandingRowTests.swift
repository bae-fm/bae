import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// What a row draws for the import that owns its candidate. The step the
/// import is at is core's answer and core tests it; these are about what each
/// step draws.
@Suite("Rows of an import in flight", .serialized)
@MainActor
struct ImportStandingRowTests {
    private static let rowSize = NSSize(width: 340, height: 80)

    /// The clock it draws instead is the `import-row-queued` shot's.
    @Test("a queued import's row keeps its two lines and draws no bar")
    func queuedTriageRow() async throws {
        let row = PreviewData.triageRowPrefilledFromTags
        let idle = try await drawn(triage: row, step: nil)
        let queued = try await drawn(
            triage: row,
            step: .preparing(step: .queued)
        )

        #expect(queued.bars == 0)
        #expect(queued.height == idle.height)
    }

    @Test(
        "a running import's row draws its bar on a line of its own",
        arguments: [
            BridgeImportStep.preparing(step: .validatingSourceFiles),
            .running(phase: .finalizing),
        ]
    )
    func runningTriageRow(step: BridgeImportStep) async throws {
        let row = PreviewData.triageRowPrefilledFromTags
        let idle = try await drawn(triage: row, step: nil)
        let running = try await drawn(triage: row, step: step)

        #expect(running.bars == 1)
        #expect(running.height > idle.height)
    }

    @Test("a queued import's Done row draws no bar")
    func queuedDoneRow() async throws {
        let row = PreviewData.triageRowDoneFromTags
        let idle = try await drawn(done: row, step: nil)
        let queued = try await drawn(done: row, step: .preparing(step: .queued))
        let running = try await drawn(
            done: row,
            step: .running(phase: .readingFiles)
        )

        #expect(queued.bars == 0)
        #expect(queued.height == idle.height)
        #expect(running.bars == 1)
    }

    /// The pane says it is waiting in a line the `import-pane-queued` shot
    /// shows.
    @Test("a queued import's pane draws no bar")
    func queuedPane() async throws {
        #expect(try await drawn(pane: .preparing(step: .queued)).bars == 0)
    }

    @Test(
        "a running import's pane draws its phase and bar",
        arguments: [
            BridgeImportStep.preparing(step: .validatingSourceFiles),
            .running(phase: .finalizing),
        ]
    )
    func runningPane(step: BridgeImportStep) async throws {
        #expect(try await drawn(pane: step).bars == 1)
    }

    private struct Drawn {
        let bars: Int
        let height: CGFloat
    }

    private func drawn(
        triage row: BridgeTriageRow,
        step: BridgeImportStep?
    ) async throws -> Drawn {
        var row = row
        row.live = BridgeCandidateLiveState(
            identifying: nil,
            readingPick: false,
            import: step,
            actions: [],
            standing: .notLookedUp,
            badge: nil
        )
        return try await drawn(
            MacImportRow(
                row: row,
                coverContent: nil,
                isGroupMember: false
            )
        )
    }

    private func drawn(
        done row: BridgeTriageRow,
        step: BridgeImportStep?
    ) async throws -> Drawn {
        var row = row
        row.live = BridgeCandidateLiveState(
            identifying: nil,
            readingPick: false,
            import: step,
            actions: [],
            standing: nil,
            badge: nil
        )
        return try await drawn(
            MacImportRow(
                row: row,
                coverContent: nil,
                isGroupMember: false
            )
        )
    }

    private func drawn(pane step: BridgeImportStep) async throws
        -> Drawn
    {
        try await drawn(
            ImportingCandidatePane(
                candidate: PreviewData.folderCandidates[0],
                step: step,
                coverContent: nil,
                onOpenImages: { _, _ in },
                onOpenDocument: { _, _ in },
                onPreview: { _ in },
                onStopPreview: {},
                previewingTarget: nil
            )
            .frame(height: 400),
            height: 400
        )
    }

    private func drawn(_ row: some View, height: CGFloat = rowSize.height)
        async throws -> Drawn
    {
        let size = NSSize(width: Self.rowSize.width, height: height)
        return try await SnapshotTestSupport.withHostedWindow(
            row
                .candidateReaderPreviewEnvironment()
                .environment(ImageStore.stub())
                .preferredColorScheme(.light)
                .background(.white)
                .frame(width: size.width),
            size: size
        ) { _, host in
            try await SnapshotTestSupport.settle(host)
            return Drawn(
                bars: SnapshotTestSupport.descendants(of: host)
                    .filter { $0 is ProgressTrackNSView }
                    .count,
                height: host.fittingSize.height
            )
        }
    }
}
