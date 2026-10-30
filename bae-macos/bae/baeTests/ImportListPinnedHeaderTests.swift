import BaeKit
import Foundation
import Testing

@testable import bae

/// The header pinned over the top of the import list is the header of the
/// group the row on top is listed under, read from that row, never the last
/// header the table happened to have loaded above it.
@Suite("Import list pinned header")
struct ImportListPinnedHeaderTests {
    @Test("a member pins its group's header once the header's page lands")
    func memberPinsItsHeaderAfterItsPageLands() {
        // The rows on top have landed; the page holding their header, far
        // above, has not.
        var landed = Self.rows(in: 50..<81)
        let loading = ImportListPinnedHeader.over(top: 60) { landed[$0] }
        #expect(loading == ImportListPinnedHeader(position: 0, header: nil))

        landed[0] = Self.list[0]
        let pinned = ImportListPinnedHeader.over(top: 60) { landed[$0] }
        #expect(
            pinned == ImportListPinnedHeader(position: 0, header: Self.list[0])
        )
    }

    @Test("a release in no group pins nothing, whatever header is above it")
    func looseReleasePinsNothing() {
        let landed = Self.rows(in: 0..<81)
        #expect(ImportListPinnedHeader.over(top: 80) { landed[$0] } == nil)
    }

    @Test("a row that has not landed pins nothing")
    func unlandedRowPinsNothing() {
        let landed = Self.rows(in: 0..<50)
        #expect(ImportListPinnedHeader.over(top: 60) { landed[$0] } == nil)
    }

    @Test("a header on top pins itself")
    func headerOnTopPinsItself() {
        let landed = Self.rows(in: 0..<50)
        #expect(
            ImportListPinnedHeader.over(top: 0) { landed[$0] }
                == ImportListPinnedHeader(position: 0, header: Self.list[0])
        )
    }

    @Test("the group's last row on top takes its header away with it")
    func lastMemberOnTopEndsTheGroup() {
        let landed = Self.rows(in: 0..<81)
        let last = ImportListPinnedHeader.over(top: 79) { landed[$0] }
        #expect(last?.ends(below: 79) { landed[$0] } == true)
        let inside = ImportListPinnedHeader.over(top: 60) { landed[$0] }
        #expect(inside?.ends(below: 60) { landed[$0] } == false)
    }
}

// MARK: - Fixtures

extension ImportListPinnedHeaderTests {
    /// A group's header at 0, its 79 members under it, and a release in no
    /// group after them.
    private static let list: [BridgeImportListItem] =
        [
            PreviewData.groupHeaderItem(
                key: BridgeFolderReleaseDecisionKey(
                    watchedFolderPath: "/library",
                    relativeFolderPath: "Group"
                ),
                name: "Group",
                entryCount: 79
            )
        ]
        + (1..<80).map { member(index: $0, headerPosition: 0) }
        + [member(index: 80, headerPosition: nil)]

    /// The rows of `list` at `positions`, as the pages holding them landed.
    private static func rows(
        in positions: Range<Int>
    ) -> [Int: BridgeImportListItem] {
        Dictionary(uniqueKeysWithValues: positions.map { ($0, list[$0]) })
    }

    private static func member(
        index: Int,
        headerPosition: UInt64?
    ) -> BridgeImportListItem {
        PreviewData.candidateItem(
            BridgeTriageRow(
                candidateKey: "/library/Group/Release \(index)",
                folderName: "Release \(index)",
                watchedFolderPath: "/library",
                displayPath: "Group/Release \(index)",
                actionable: true,
                placement: .pending,
                live: BridgeCandidateLiveState(
                    identifying: nil,
                    readingPick: false,
                    import: nil,
                    actions: [],
                    standing: .notLookedUp,
                    badge: nil
                ),
                metadataSummary: nil,
                cover: nil,
                importStatus: nil,
                upload: nil,
                metadataProvenance: nil,
                reading: .unidentified,
                selected: false
            ),
            headerPosition: headerPosition
        )
    }
}
