import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// What a row draws for the import that owns its candidate. Where the import
/// stands is core's answer and core tests it; these are about what each
/// standing draws.
@Suite("Rows of an import in flight", .serialized)
@MainActor
struct ImportStandingRowTests {
    private static let rowSize = NSSize(width: 340, height: 80)

    @Test("a queued import's row keeps its two lines and draws no bar")
    func queuedTriageRow() async throws {
        let row = PreviewData.triageRowPrefilledFromTags
        let idle = try await drawn(triage: row, standing: nil)
        let queued = try await drawn(triage: row, standing: .queued)

        #expect(queued.bars == 0)
        #expect(queued.height == idle.height)
        #expect(queued.pixels != idle.pixels, "the queued row draws its clock")
    }

    @Test(
        "a running import's row draws its bar on a line of its own",
        arguments: [BridgeImportStanding.running, .writing]
    )
    func runningTriageRow(standing: BridgeImportStanding) async throws {
        let row = PreviewData.triageRowPrefilledFromTags
        let idle = try await drawn(triage: row, standing: nil)
        let running = try await drawn(triage: row, standing: standing)

        #expect(running.bars == 1)
        #expect(running.height > idle.height)
    }

    @Test("a queued import's Done row draws no bar")
    func queuedDoneRow() async throws {
        let row = PreviewData.importedRowFromTags
        let idle = try await drawn(done: row, standing: nil)
        let queued = try await drawn(done: row, standing: .queued)
        let running = try await drawn(done: row, standing: .running)

        #expect(queued.bars == 0)
        #expect(queued.height == idle.height)
        #expect(running.bars == 1)
    }

    @Test("a queued import's pane says it is waiting and draws no bar")
    func queuedPane() async throws {
        let queued = try await drawn(pane: .queued)

        #expect(queued.bars == 0)
        let lines =
            try await SnapshotTestSupport.recognizedText(
                in: queued.pixels,
                languages: ["en-US"]
            )
            .map(\.text)
        #expect(lines.carrying("Waiting to import"))
    }

    @Test(
        "a running import's pane draws its phase and bar",
        arguments: [BridgeImportStanding.running, .writing]
    )
    func runningPane(standing: BridgeImportStanding) async throws {
        #expect(try await drawn(pane: standing).bars == 1)
    }

    private struct Drawn {
        let bars: Int
        let height: CGFloat
        let pixels: Data
    }

    private func drawn(
        triage row: BridgeTriageRow,
        standing: BridgeImportStanding?
    ) async throws -> Drawn {
        var row = row
        row.live = BridgeCandidateLiveState(
            identification: nil,
            import: standing,
            actions: [],
            standing: .notLookedUp,
            badge: nil
        )
        return try await drawn(
            TriageRowView(
                row: row,
                coverContent: nil,
                isGroupMember: false
            )
        )
    }

    private func drawn(
        done row: BridgeImportedRow,
        standing: BridgeImportStanding?
    ) async throws -> Drawn {
        var row = row
        row.live = BridgeCandidateLiveState(
            identification: nil,
            import: standing,
            actions: [],
            standing: nil,
            badge: nil
        )
        return try await drawn(
            ImportedRowView(
                row: row,
                uploadObservation: nil,
                onReveal: {}
            )
        )
    }

    private func drawn(pane standing: BridgeImportStanding) async throws
        -> Drawn
    {
        try await drawn(
            ImportingCandidatePane(
                candidate: PreviewData.folderCandidates[0],
                standing: standing,
                runtime: nil,
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
                height: host.fittingSize.height,
                pixels: try await SnapshotTestSupport.capturePNG(
                    host,
                    size: size
                )
            )
        }
    }
}
