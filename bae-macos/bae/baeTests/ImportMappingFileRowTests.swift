import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

@Suite("Import mapping file row")
struct ImportMappingFileRowTests {
    @MainActor
    @Test("an excluded audio file keeps its Put Back action")
    func excludedAudioKeepsItsRoleAction() async throws {
        let choice = RoleChoiceRecorder()
        let tableWidth: CGFloat = 520
        let size = NSSize(width: tableWidth, height: 60)
        let (window, host) = SnapshotTestSupport.hostInWindow(
            ImportMappingFileRow(
                file: excludedAudioFile,
                columns:
                    ImportMappingColumns.resolved(
                        tableWidth: tableWidth
                    )
                    .files,
                previewingPath: nil,
                evidence: [],
                actions: actions(recording: choice)
            )
            .frame(width: tableWidth, height: size.height),
            size: size
        )
        host.layoutSubtreeIfNeeded()
        await Task.yield()
        host.layoutSubtreeIfNeeded()

        let buttons = SnapshotTestSupport.descendants(of: host)
            .compactMap {
                $0 as? NSButton
            }
        try #require(buttons.count == 1)
        let button = try #require(buttons.first)
        button.performClick(nil)
        await Task.yield()

        #expect(choice.fileId == "excluded.flac")
        #expect(choice.choice == .audio)
        withExtendedLifetime(window) {}
    }

    private var excludedAudioFile: BridgeMappingFile {
        BridgeMappingFile(
            fileId: "excluded.flac",
            name: "excluded.flac",
            size: 24_000_000,
            localPath: "/tmp/excluded.flac",
            durationMs: 180_000,
            audioFormat: MappingFixtures.audioFormat,
            role: .other,
            alternatives: [.audio, .notATrack],
            roleChoice: .notATrack
        )
    }

    private func actions(
        recording choice: RoleChoiceRecorder
    ) -> ImportMappingActions {
        ImportMappingActions(
            setRole: {
                choice.fileId = $0
                choice.choice = $1
            },
            bindSheet: { _, _ in },
            setSheetDisc: { _, _ in },
            openDocument: { _, _ in },
            openImages: { _, _ in },
            preview: { _ in },
            stopPreview: {},
            editTrack: { _ in },
            setTrackArtists: { _, _ in },
            chooseFile: { _, _ in },
            drop: { _ in },
            exclude: { _ in },
        )
    }
}

private final class RoleChoiceRecorder {
    var fileId: String?
    var choice: BridgeFileRoleChoice?
}
