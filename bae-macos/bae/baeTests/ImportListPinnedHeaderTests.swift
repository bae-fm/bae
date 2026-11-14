import BaeKit
import Foundation
import Testing

@testable import bae

/// The header pinned over the top of the import list is the header of the
/// group the row on top is listed under, read from that row's header key,
/// never the last header the table happened to have loaded above it.
@Suite("Import list pinned header")
struct ImportListPinnedHeaderTests {
    @Test("a member pins its group's header once a window delivers it")
    func memberPinsItsHeaderOnceDelivered() {
        // The rows on top have landed; the window holding their header, far
        // above, has not.
        var landed = Self.rows(in: 50..<81)
        let loading = ImportListPinnedHeader.over(
            top: 60,
            item: { landed[$0] },
            header: { _ in nil }
        )
        #expect(
            loading == ImportListPinnedHeader(key: Self.header.id, header: nil)
        )

        landed[0] = Self.header
        let pinned = ImportListPinnedHeader.over(
            top: 60,
            item: { landed[$0] },
            header: { key in landed.values.first { $0.id == key } }
        )
        #expect(
            pinned
                == ImportListPinnedHeader(
                    key: Self.header.id,
                    header: Self.header
                )
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
                == ImportListPinnedHeader(
                    key: Self.header.id,
                    header: Self.header
                )
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

    /// The windows a list holds, the third starting inside the group: the
    /// pinned header is the header that window delivers beside its rows,
    /// though no window holds the header's own row.
    @MainActor
    @Test("a window that starts inside a group pins the header it delivers")
    func windowStartingInsideTheGroupPinsItsLeadingHeader() async throws {
        let header = Self.header
        let list = PaginatedList<BridgeImportListItem>(
            queries: .fixed(
                Self.list,
                answers: UnansweredCount(),
                extra: BridgeListCountExtra.import(narrowed: nil),
                leadingHeader: { position in
                    position > 0 && position < 80 ? header : nil
                }
            ),
            onError: { _ in }
        )
        await list.reopen(
            at: BridgeListQueries<BridgeImportListItem>.fixedKey(60),
            position: 60
        )

        #expect(list.row(of: header.id) == nil)
        let pinned = ImportListPinnedHeader.over(
            top: 60,
            item: { list.row(at: $0) },
            header: { list.row(of: $0) ?? list.leadingHeader(of: $0) }
        )
        #expect(
            pinned == ImportListPinnedHeader(key: header.id, header: header)
        )
    }

    private static func pinned(
        over top: Int,
        landed: [Int: BridgeImportListItem]
    ) -> ImportListPinnedHeader? {
        ImportListPinnedHeader.over(
            top: top,
            item: { landed[$0] },
            header: { key in landed.values.first { $0.id == key } }
        )
    }
}

// MARK: - Fixtures

extension ImportListPinnedHeaderTests {
    /// A group's header at 0, its 79 members under it, and a release in no
    /// group after them.
    private static let header = PreviewData.groupHeaderItem(
        key: BridgeFolderReleaseDecisionKey(
            watchedFolderPath: Data("/library".utf8),
            relativeFolder: Data("Group".utf8)
        ),
        name: "Group",
        entryCount: 79
    )

    private static let list: [BridgeImportListItem] =
        [header]
        + (1..<80).map { member(index: $0, headerKey: header.id) }
        + [member(index: 80, headerKey: nil)]

    /// The rows of `list` at `positions`, as the pages holding them landed.
    private static func rows(
        in positions: Range<Int>
    ) -> [Int: BridgeImportListItem] {
        Dictionary(uniqueKeysWithValues: positions.map { ($0, list[$0]) })
    }

    private static func member(
        index: Int,
        headerKey: String?
    ) -> BridgeImportListItem {
        PreviewData.candidateItem(
            BridgeTriageRow(
                candidateKey: Data("/library/Group/Release \(index)".utf8),
                folderName: "Release \(index)",
                watchedFolderPath: Data("/library".utf8),
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
            headerKey: headerKey
        )
    }
}
