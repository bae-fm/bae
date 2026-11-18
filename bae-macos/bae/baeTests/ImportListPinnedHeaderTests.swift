import BaeKit
import Foundation
import Testing

@testable import bae

/// The header pinned over the top of the import list is the header of the
/// group the row on top is listed under, which that row carries, never the
/// last header the table happened to have loaded above it.
@Suite("Import list pinned header")
struct ImportListPinnedHeaderTests {
    @Test("a member pins the header it carries, whatever rows the list holds")
    func memberPinsItsHeader() {
        // The list holds the rows on top, but not the header's own row, far
        // above.
        let landed = Self.rows(in: 50..<81)
        #expect(
            Self.pinned(over: 60, landed: landed)
                == ImportListPinnedHeader(header: Self.header)
        )
    }

    @Test("a release in no group pins nothing, whatever header is above it")
    func looseReleasePinsNothing() {
        let landed = Self.rows(in: 0..<81)
        #expect(Self.pinned(over: 80, landed: landed) == nil)
    }

    @Test("a row that has not landed pins nothing")
    func unlandedRowPinsNothing() {
        let landed = Self.rows(in: 0..<50)
        #expect(Self.pinned(over: 60, landed: landed) == nil)
    }

    @Test("a header on top pins itself")
    func headerOnTopPinsItself() {
        let landed = Self.rows(in: 0..<50)
        #expect(
            Self.pinned(over: 0, landed: landed)
                == ImportListPinnedHeader(header: Self.header)
        )
    }

    @Test("the group's last row on top takes its header away with it")
    func lastMemberOnTopEndsTheGroup() {
        let landed = Self.rows(in: 0..<81)
        let last = Self.pinned(over: 79, landed: landed)
        #expect(last?.ends(below: 79) { landed[$0] } == true)
        let inside = Self.pinned(over: 60, landed: landed)
        #expect(inside?.ends(below: 60) { landed[$0] } == false)
    }

    private static func pinned(
        over top: Int,
        landed: [Int: BridgeImportListItem]
    ) -> ImportListPinnedHeader? {
        ImportListPinnedHeader.over(top: top, item: { landed[$0] })
    }
}

// MARK: - Fixtures

extension ImportListPinnedHeaderTests {
    /// A group's header at 0, its 79 members under it, and a release in no
    /// group after them.
    private static let header = PreviewData.groupHeader(
        key: BridgeFolderReleaseDecisionKey(
            watchedFolderPath: Data("/library".utf8),
            relativeFolder: Data("Group".utf8)
        ),
        name: "Group",
        entryCount: 79
    )

    private static let list: [BridgeImportListItem] =
        [.groupHeader(header: header)]
        + (1..<80).map { member(index: $0, header: header) }
        + [member(index: 80, header: nil)]

    /// The rows of `list` at `positions`, as the pages holding them landed.
    private static func rows(
        in positions: Range<Int>
    ) -> [Int: BridgeImportListItem] {
        Dictionary(uniqueKeysWithValues: positions.map { ($0, list[$0]) })
    }

    private static func member(
        index: Int,
        header: BridgeImportGroupHeader?
    ) -> BridgeImportListItem {
        PreviewData.candidateItem(
            BridgeTriageRow(
                candidateKey: Data("/library/Group/Release \(index)".utf8),
                folderName: "Release \(index)",
                watchedFolderPath: Data("/library".utf8),
                actionable: true,
                placement: .pending,
                live: BridgeCandidateLiveState(
                    identifying: nil,
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
                reading: .unidentified
            ),
            header: header
        )
    }
}
