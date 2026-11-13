import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// A symbol a control or the words beside it already name is not an element
/// of its own for a screen reader, and one that says something nothing else
/// says is named for it. Read through the accessibility API, as a screen
/// reader reads: SwiftUI builds its elements only for such a client.
@MainActor
@Suite("Redundant icons hidden from screen readers")
struct RedundantIconsHiddenTests {
    /// The labels of the image elements `view` lists.
    private func images<V: View>(in view: V) async throws -> [String] {
        try await elements(in: view).filter { $0.role == "AXImage" }
            .map { $0.description ?? "(no label)" }
    }

    private func elements<V: View>(in view: V) async throws
        -> [AccessibilityTree.Node]
    {
        let size = NSSize(width: 400, height: 80)
        return try await SnapshotTestSupport.withHostedWindow(
            view.frame(width: size.width, height: size.height),
            size: size
        ) { window, host in
            try await SnapshotTestSupport.settle(host)
            let tree = try AccessibilityTree.read(
                of: window,
                subject: "a hosted view"
            )
            return tree.nodes
        }
    }

    @Test("the info tip's glyph is not an element beside its label")
    func infoTip() async throws {
        let found = try await images(
            in: MacInfoTip(label: "About MusicBrainz") { Text("Words") }
        )
        #expect(found.isEmpty, "listed images \(found)")
    }

    @Test("the close button's glyph is not an element beside its label")
    func closeButton() async throws {
        let found = try await images(in: MacCloseButton(onClose: {}))
        #expect(found.isEmpty, "listed images \(found)")
    }

    @Test("the folder glyph is not an element beside the folder's name")
    func folderNameLine() async throws {
        let found = try await images(
            in: ImportFolderNameLine(folderName: "Blue Room") { EmptyView() }
        )
        #expect(found.isEmpty, "listed images \(found)")
    }

    @Test("the playing glyph names itself, since nothing else says it")
    func playingGlyph() async throws {
        let found = try await images(
            in: MacTrackRow(
                track: PreviewData.previewTrack(
                    title: "Harbor Lights",
                    position: "1",
                    durationMs: 272_000
                ),
                artist: nil,
                positionWidth: MacTrackRow.leadingSlotMinWidth,
                isCurrent: true,
                isPlaying: true,
                onPlay: {},
                onTogglePlayPause: {}
            )
        )
        #expect(found == ["Now playing"], "listed images \(found)")
    }

    @Test("a failed signal's glyph names the failure, since nothing else does")
    func failedGlyphs() async throws {
        let signal = try await images(in: IdentifierWarning())
        #expect(signal == ["Failed"], "listed images \(signal)")
        let section = try await images(
            in: FindOnlineSectionGlyphView(glyph: .failed)
        )
        #expect(section == ["Failed"], "listed images \(section)")
    }

    @Test("a queue row's cancel control is named for what it cancels")
    func queueRowCancel() async throws {
        let nodes = try await elements(
            in: QueueRow(
                icon: "pin",
                createdAt: nil,
                cancel: .init(help: "Cancel", action: {})
            ) {
                Text("Blue Room")
            } below: {
                EmptyView()
            }
        )
        let images = nodes.filter { $0.role == "AXImage" }
        #expect(images.isEmpty, "listed images \(images.map(\.description))")
        #expect(
            nodes.contains {
                $0.role == "AXButton" && $0.description == "Cancel"
            },
            "listed \(nodes.map { ($0.role, $0.description) })"
        )
    }
}
