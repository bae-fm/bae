import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// What scrolling the import list end to end costs, step by step, with every
/// row kind in one list: Found's rows with covers under folder group headers,
/// broken folders' rows and Done rows. A measurement, run on demand with
/// `BAE_MEASURE` set; it prints its numbers and checks nothing.
@MainActor
@Suite("Import list scroll cost")
struct ImportListScrollCostTests {
    /// The sidebar's width, and a tall window's height.
    static let size = NSSize(width: 360, height: 800)

    @Test(
        "scrolling the list end to end, its rows hosted SwiftUI views",
        .enabled(if: ProcessInfo.processInfo.environment["BAE_MEASURE"] != nil)
    )
    func hostedRows() async throws {
        let uiStore = UiStore()
        uiStore.setImportCandidateTab(.pending)
        let store = PreviewData.importTabScene().store
        let items = ScrollCostFixture.items()
        let images = try await ScrollCostFixture.warmedImages(for: items)
        let slot = ImportListSlot.preview(
            importStore: store,
            uiStore: uiStore,
            items: items
        )
        try await ImportListHosting.withList(
            store: store,
            slot: slot,
            uiStore: uiStore,
            imageStore: images,
            size: Self.size
        ) { table in
            #expect(table.numberOfRows == items.count)
            ScrollCost.measure(table, named: "hosted rows")
        }
    }
}

/// The rows the measurement scrolls: 30 folder group headers over 200 of
/// Found's rows with covers, then 40 broken folders' rows and 30 Done rows.
/// Rows of three tabs in one list is not what a tab shows; it puts every row
/// kind's drawing in one pass.
@MainActor
enum ScrollCostFixture {
    static let groups = 30
    static let foundRows = 200
    static let invalidRows = 40
    static let doneRows = 30
    /// How many different covers the rows cycle through.
    static let coverCount = 24

    static func items() -> [BridgeImportListItem] {
        foundItems()
            + (0..<invalidRows).map(invalidItem)
            + (0..<doneRows).map(doneItem)
    }

    /// Found's rows under their groups' headers.
    private static func foundItems() -> [BridgeImportListItem] {
        let found = [
            PreviewData.triageRowUnidentified,
            PreviewData.triageRowPrefilledFromTags,
            PreviewData.triageRowIdentifiedOnline,
            PreviewData.triageRowIdentifiedSeveralMatches,
            PreviewData.triageRowIdentified,
            PreviewData.triageRowNoMatch,
            PreviewData.triageRowIdentifying,
        ]
        var items: [BridgeImportListItem] = []
        var row = 0
        for group in 0..<groups {
            // The first groups take one row more, so the 200 rows fill all 30.
            let members =
                foundRows / groups + (group < foundRows % groups ? 1 : 0)
            let header = PreviewData.groupHeaderItem(
                key: BridgeFolderReleaseDecisionKey(
                    watchedFolderPath: PreviewData.importWatchedFolder.path,
                    relativeFolder: Data("Collection \(group)".utf8)
                ),
                name: "Collection \(group)",
                entryCount: UInt32(members)
            )
            items.append(header)
            for _ in 0..<members {
                var member = found[row % found.count]
                member.candidateKey = key("found", row)
                member.folderName = "Release \(row)"
                member.displayPath = "Collection \(group)/Release \(row)"
                member.cover = cover(row)
                items.append(
                    PreviewData.candidateItem(
                        member,
                        headerKey: header.id
                    )
                )
                row += 1
            }
        }
        return items
    }

    private static func invalidItem(_ index: Int) -> BridgeImportListItem {
        var invalid =
            PreviewData.invalidCandidates[
                index % PreviewData.invalidCandidates.count
            ]
        invalid.candidateKey = key("invalid", index)
        invalid.folderPath = invalid.candidateKey
        invalid.sourceFolderName = "Broken \(index)"
        invalid.displayPath = "Broken \(index)"
        if invalid.cover != nil {
            invalid.cover = cover(index)
        }
        return PreviewData.invalidItem(invalid)
    }

    private static func doneItem(_ index: Int) -> BridgeImportListItem {
        var done = PreviewData.triageRowDoneImported
        done.candidateKey = key("done", index)
        done.folderName = "Imported \(index)"
        done.displayPath = "Imported \(index)"
        done.cover = cover(index)
        return PreviewData.candidateItem(done)
    }

    /// An image store already holding every cover the rows draw, decoded at
    /// the rows' size for both display scales, so no step decodes one.
    static func warmedImages(
        for items: [BridgeImportListItem]
    ) async throws -> ImageStore {
        let images = ImageStore()
        let covers = Array(
            Set(
                items.compactMap { item -> ImageContent? in
                    switch item {
                    case .candidate(_, let row, _):
                        row.cover.map(ImageContent.init(bridge:))
                    case .invalid(_, let invalid, _):
                        invalid.cover.map(ImageContent.init(bridge:))
                    case .groupHeader:
                        nil
                    }
                }
            )
        )
        for scale: CGFloat in [1, 2] {
            await images.warm(
                covers,
                pointSize: MacImportRow.coverPointSize,
                displayScale: scale
            )
            for cover in covers {
                try #require(
                    images.cachedImage(
                        cover,
                        pointSize: MacImportRow.coverPointSize,
                        displayScale: scale
                    ) != nil,
                    "\(cover) is decoded before the scroll"
                )
            }
        }
        return images
    }

    private static func key(_ kind: String, _ index: Int) -> Data {
        Data("/Music/Scroll/\(kind) \(index)".utf8)
    }

    private static func cover(_ index: Int) -> BridgeCoverImageSource {
        .local(
            file: PreviewData.previewArtFile(
                "Scroll cover \(index % coverCount)"
            )
        )
    }
}

/// Scrolling a table end to end in three passes, each step timed from the
/// clip view's move through the table's layout and display: steps of one
/// row from the top with cells made as rows enter, the same again over cells
/// reused, then steps of one viewport, every visible cell reused per step.
@MainActor
enum ScrollCost {
    /// One 120 Hz frame.
    static let frame: Duration = .microseconds(8_333)

    static func measure(_ table: NSTableView, named name: String) {
        guard let scrollView = table.enclosingScrollView else {
            Issue.record("the table is not in a scroll view")
            return
        }
        let clip = scrollView.contentView
        let rowHeight = table.rect(ofRow: 0).height
        let viewport = clip.bounds.height
        let end = table.bounds.height - viewport
        let rowSteps = Array(stride(from: 0, through: end, by: rowHeight))
        let viewportRows = (viewport / rowHeight).rounded(.down)
        let viewportSteps = Array(
            stride(from: 0, through: end, by: viewportRows * rowHeight)
        )
        print(
            "scroll cost [\(name)]: \(table.numberOfRows) rows of \(rowHeight) pt, viewport \(viewport) pt, load \(loadAverage())"
        )
        report(
            name,
            "pass 1, one row a step, cells made",
            steps(rowSteps, table, scrollView)
        )
        report(
            name,
            "pass 2, one row a step, cells reused",
            steps(rowSteps, table, scrollView)
        )
        report(
            name,
            "pass 3, one viewport a step",
            steps(viewportSteps, table, scrollView),
            rowsPerStep: Int(viewportRows)
        )
        print("scroll cost [\(name)]: load after \(loadAverage())")
    }

    /// Each step's time, from the top: a step moves the clip view to its
    /// offset and has the table lay out and draw what it shows there.
    private static func steps(
        _ offsets: [CGFloat],
        _ table: NSTableView,
        _ scrollView: NSScrollView
    ) -> [Duration] {
        let clock = ContinuousClock()
        move(to: 0, table, scrollView)
        return offsets.dropFirst()
            .map { offset in
                autoreleasepool {
                    clock.measure { move(to: offset, table, scrollView) }
                }
            }
    }

    private static func move(
        to offset: CGFloat,
        _ table: NSTableView,
        _ scrollView: NSScrollView
    ) {
        let clip = scrollView.contentView
        clip.scroll(to: NSPoint(x: 0, y: offset))
        scrollView.reflectScrolledClipView(clip)
        table.layoutSubtreeIfNeeded()
        scrollView.displayIfNeeded()
    }

    private static func report(
        _ name: String,
        _ pass: String,
        _ times: [Duration],
        rowsPerStep: Int? = nil
    ) {
        let sorted = times.sorted()
        guard !sorted.isEmpty else {
            Issue.record("\(pass) took no steps")
            return
        }
        let median = sorted[sorted.count / 2]
        let p95 = sorted[min(sorted.count - 1, sorted.count * 95 / 100)]
        let longest = sorted[sorted.count - 1]
        let overFrame = sorted.filter { $0 > frame }.count
        var line =
            "scroll cost [\(name)] \(pass): \(sorted.count) steps, median \(ms(median)), p95 \(ms(p95)), max \(ms(longest)), \(overFrame) over 8.3 ms"
        if let rowsPerStep, rowsPerStep > 0 {
            line += ", per reused cell \(ms(median / rowsPerStep)) (median)"
        }
        print(line)
    }

    private static func ms(_ duration: Duration) -> String {
        let (seconds, attoseconds) = duration.components
        let value =
            Double(seconds) * 1_000 + Double(attoseconds)
            / 1_000_000_000_000_000
        return String(format: "%.2f ms", value)
    }

    private static func loadAverage() -> String {
        var loads = [Double](repeating: 0, count: 3)
        guard getloadavg(&loads, 3) == 3 else { return "unknown" }
        return loads.map { String(format: "%.2f", $0) }.joined(separator: " ")
    }
}
