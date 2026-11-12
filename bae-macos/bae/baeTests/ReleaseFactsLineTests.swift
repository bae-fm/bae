import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// The album header's release facts: the pressing on one line and its labels
/// on another, the labels as core grouped them.
@MainActor
@Suite("The album header's release facts")
struct ReleaseFactsLineTests {
    private static func release(_ albumId: String) throws -> ReleaseDetail {
        let album = AlbumDetail(
            bridge: try #require(PreviewData.albumDetails[albumId])
        )
        return album.primaryRelease
    }

    @Test("the labels line shows each name group once, then its numbers")
    func theLabelsLineShowsCoreGroups() throws {
        let release = try Self.release("a-21")
        let separator = localizedCoreString("core.audio.list_separator")
        let within = localizedCoreString("core.label.list_separator")

        #expect(!release.pressingLine.isEmpty)
        #expect(
            release.labelsLine
                == ["Label A\(within)Label B", "AB 100", "Label C", "CL 719"]
                .joined(separator: separator)
        )
        #expect(!release.pressingLine.contains("Label"))
    }

    @Test("the pressing line names the year only when core hands one over")
    func thePressingLineNamesTheYearOnlyWhenCoreHandsOneOver() {
        let separator = localizedCoreString("core.audio.list_separator")
        let reissue = ReleaseDetail(
            bridge: makeBridgeRelease(distinctYear: 2001)
        )
        let original = ReleaseDetail(bridge: makeBridgeRelease())

        #expect(reissue.pressingLine.hasPrefix("2001\(separator)"))
        #expect(!original.pressingLine.contains("2024"))
        #expect(!original.pressingLine.hasPrefix(separator))
    }

    @Test("the facts draw as two lines, each on one line of its own")
    func theFactsDrawAsTwoLines() throws {
        let release = try Self.release("a-21")
        func height(_ view: ReleaseFactsLine) -> CGFloat {
            let host = NSHostingView(
                rootView: view.frame(width: 160).importPreviewEnvironment()
            )
            return host.fittingSize.height
        }
        let both = ReleaseFactsLine(
            pressingLine: release.pressingLine,
            labelsLine: release.labelsLine,
            records: []
        )
        let pressingOnly = ReleaseFactsLine(
            pressingLine: release.pressingLine,
            labelsLine: "",
            records: []
        )
        let long = ReleaseFactsLine(
            pressingLine: release.pressingLine,
            labelsLine: String(repeating: release.labelsLine, count: 6),
            records: []
        )

        #expect(both.shownLines.count == 2)
        #expect(pressingOnly.shownLines.count == 1)
        #expect(height(both) > height(pressingOnly))
        // A long labels line is cut short rather than wrapping onto more.
        #expect(abs(height(long) - height(both)) < 1)
    }
}
