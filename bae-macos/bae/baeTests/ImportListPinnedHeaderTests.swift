import BaeKit
import Foundation
import Testing

@testable import bae

/// The header pinned over the top of the import list is the header of the
/// group the row on top is listed under, which that row carries, never the
/// last header the table happened to have loaded above it, and the row on
/// top says whether the group ends with it.
@Suite("Import list pinned header")
struct ImportListPinnedHeaderTests {
    @Test("a member pins the header it carries, whatever rows the list holds")
    func memberPinsItsHeader() {
        #expect(
            ImportListPinnedHeader.over(top: Self.list[60])
                == ImportListPinnedHeader(header: Self.header, ends: false)
        )
    }

    @Test("a release in no group pins nothing, whatever header is above it")
    func looseReleasePinsNothing() {
        #expect(ImportListPinnedHeader.over(top: Self.list[80]) == nil)
    }

    @Test("a row that has not landed pins nothing")
    func unlandedRowPinsNothing() {
        #expect(ImportListPinnedHeader.over(top: nil) == nil)
    }

    @Test("an open header on top pins itself over its members")
    func headerOnTopPinsItself() {
        #expect(
            ImportListPinnedHeader.over(top: Self.list[0])
                == ImportListPinnedHeader(header: Self.header, ends: false)
        )
    }

    @Test("a header that shows no member on top ends its group")
    func foldedHeaderEndsItsGroup() {
        let folded = PreviewData.groupHeader(
            key: Self.header.group.key,
            name: "Group",
            expanded: false,
            entryCount: 79,
            showsNoMember: true
        )
        #expect(
            ImportListPinnedHeader.over(top: .groupHeader(header: folded))?.ends
                == true
        )
    }

    @Test("the group's last row on top takes its header away with it")
    func lastMemberOnTopEndsTheGroup() {
        #expect(ImportListPinnedHeader.over(top: Self.list[79])?.ends == true)
        #expect(ImportListPinnedHeader.over(top: Self.list[60])?.ends == false)
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
        + (1..<80).map { member(index: $0, header: header, last: $0 == 79) }
        + [member(index: 80, header: nil, last: false)]

    private static func member(
        index: Int,
        header: BridgeImportGroupHeader?,
        last: Bool
    ) -> BridgeImportListItem {
        PreviewData.candidateItem(
            BridgeTriageRow(
                candidateKey: Data("/library/Group/Release \(index)".utf8),
                folderName: "Release \(index)",
                watchedFolderPath: Data("/library".utf8),
                actionable: true,
                placement: .pending,
                live: PreviewData.triageLive([]),
                metadataSummary: nil,
                cover: nil,
                importStatus: nil,
                metadataProvenance: nil,
                reading: .unidentified
            ),
            header: header,
            last: last
        )
    }
}
