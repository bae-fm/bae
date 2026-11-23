import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// The album header's release facts: the edition line and the copy line, the
/// labels as core grouped them and the country last.
@MainActor
@Suite("The album header's release facts")
struct ReleaseFactsLineTests {
    private static func release(_ albumId: String) throws -> ReleaseDetail {
        let album = AlbumDetail(
            bridge: try #require(PreviewData.albumDetails[albumId])
        )
        return album.shownRelease
    }

    @Test(
        "the edition line shows each label group with its numbers, then the country"
    )
    func theEditionLineEndsWithTheCountry() throws {
        let release = try Self.release("a-21")
        let separator = localizedCoreString("core.audio.list_separator")
        let within = localizedCoreString("core.label.list_separator")
        let country = localizedCoreString("core.pressing.country.us")

        #expect(
            release.editionLine
                == [
                    "Label A\(within)Label B", "AB 100", "Label C", "CL 719",
                    country,
                ]
                .joined(separator: separator)
        )
        #expect(!release.copyLine.contains("Label"))
        #expect(!release.copyLine.contains(country))
    }

    @Test("the edition line starts with the year only when core hands one over")
    func theEditionLineNamesTheYearOnlyWhenCoreHandsOneOver() {
        let separator = localizedCoreString("core.audio.list_separator")
        let reissue = ReleaseDetail(
            bridge: makeBridgeRelease(distinctYear: 2001)
        )
        let original = ReleaseDetail(bridge: makeBridgeRelease())

        #expect(reissue.editionLine.hasPrefix("2001"))
        #expect(!original.editionLine.contains("2024"))
        #expect(!original.editionLine.hasPrefix(separator))
    }

    @Test("the copy line holds the media, then the play time")
    func theCopyLineHoldsTheMediaThenTheTime() {
        let release = ReleaseDetail(bridge: makeBridgeRelease())

        #expect(release.copyLine.hasPrefix("CD"))
        #expect(release.copyLine.hasSuffix("45 min"))
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
            editionLine: release.editionLine,
            copyLine: release.copyLine,
            records: [],
            hasRecords: false
        )
        let editionOnly = ReleaseFactsLine(
            editionLine: release.editionLine,
            copyLine: "",
            records: [],
            hasRecords: false
        )
        let long = ReleaseFactsLine(
            editionLine: release.editionLine,
            copyLine: String(repeating: release.copyLine, count: 6),
            records: [],
            hasRecords: false
        )

        #expect(both.shownLines.count == 2)
        #expect(editionOnly.shownLines.count == 1)
        #expect(height(both) > height(editionOnly))
        // A long copy line is cut short rather than wrapping onto more.
        #expect(abs(height(long) - height(both)) < 1)
    }
}
