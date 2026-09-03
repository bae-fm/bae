import Foundation
import Testing

@testable import bae

/// The mapping table's column widths, against the width the table has to lay
/// them out in.
///
/// The one that matters is that the tracks section adds up: five columns plus
/// the action slot, never reserving an invisible slice past the pane's right
/// edge. Files rows are not columnar.
struct ImportMappingColumnsTests {
    /// Every column, the action slot, the gaps between them and the row's two
    /// leading edges, as the row lays them out.
    private func rowWidth(_ columns: ReleaseMetadataTrackColumns) -> CGFloat {
        columns.title + columns.artist + columns.source
            + ReleaseMetadataTrackColumns.side
            + ReleaseMetadataTrackColumns.track
            + ReleaseMetadataTrackColumns.length
            + ReleaseMetadataTrackColumns.action
            + ReleaseMetadataTrackColumns.spacing * 6
            + ReleaseMetadataTrackColumns.rowPadding * 2
    }

    @Test(
        arguments: [
            ReleaseMetadataTrackColumns.minimumTableWidth,
            700, 733.5, 800, 900,
            ReleaseMetadataTrackColumns.idealTableWidth,
            1200, 1600, 2400,
        ] as [CGFloat]
    )
    func theRowIsExactlyTheTableWide(width: CGFloat) {
        let columns = ReleaseMetadataTrackColumns.resolved(tableWidth: width)

        #expect(rowWidth(columns) == width)
    }

    /// Under its minimum the table stops shrinking and is laid out at that
    /// minimum instead — the pane scrolls it sideways from there.
    ///
    /// The widths are stated against the minimum rather than as numbers: the
    /// column floors have moved once already, and a literal that used to sit
    /// under the minimum quietly becomes a width the table lays out normally.
    @Test(
        arguments: [
            0,
            ReleaseMetadataTrackColumns.minimumTableWidth / 4,
            ReleaseMetadataTrackColumns.minimumTableWidth / 2,
            ReleaseMetadataTrackColumns.minimumTableWidth - 1,
        ] as [CGFloat]
    )
    func aTableTooNarrowForItsColumnsIsLaidOutAtItsMinimum(width: CGFloat) {
        let columns = ReleaseMetadataTrackColumns.resolved(tableWidth: width)

        let atMinimum = ReleaseMetadataTrackColumns.resolved(
            tableWidth: ReleaseMetadataTrackColumns.minimumTableWidth
        )

        #expect(
            rowWidth(columns)
                == ReleaseMetadataTrackColumns.minimumTableWidth
        )
        #expect(columns.source == atMinimum.source)
        #expect(columns.title == atMinimum.title)
        #expect(columns.artist == atMinimum.artist)
    }

    /// Wide enough and every Tracks column has the width it asks for, with the
    /// surplus going to Source and nowhere else.
    @Test(arguments: [0, 1, 200, 900] as [CGFloat])
    func theSurplusAboveTheIdealWidthIsAllTheSource(surplus: CGFloat) {
        let columns = ReleaseMetadataTrackColumns.resolved(
            tableWidth: ReleaseMetadataTrackColumns.idealTableWidth + surplus
        )

        #expect(columns.title == 220)
        #expect(columns.artist == 180)
        #expect(columns.source == 260 + surplus)
    }

    /// Narrowing takes from all three at once. A column that kept its width
    /// while its neighbour collapsed would be the same bug in miniature: the
    /// row still fits, and one cell has stopped saying anything.
    @Test
    func narrowingTakesFromEveryColumnThatHasGive() {
        let wide = ReleaseMetadataTrackColumns.resolved(
            tableWidth: ReleaseMetadataTrackColumns.idealTableWidth
        )
        let middle = ReleaseMetadataTrackColumns.resolved(
            tableWidth: (ReleaseMetadataTrackColumns.idealTableWidth
                + ReleaseMetadataTrackColumns.minimumTableWidth) / 2
        )
        let narrow = ReleaseMetadataTrackColumns.resolved(
            tableWidth: ReleaseMetadataTrackColumns.minimumTableWidth
        )

        #expect(middle.source < wide.source)
        #expect(middle.title < wide.title)
        #expect(middle.artist < wide.artist)
        #expect(narrow.source < middle.source)
        #expect(narrow.title < middle.title)
        #expect(narrow.artist < middle.artist)
    }

    /// Widening never narrows a column and narrowing never widens one, at every
    /// width in between — the columns follow the pane rather than jumping about
    /// as it is dragged.
    @Test
    func everyColumnFollowsTheTableWidthOneWay() {
        var previous = ReleaseMetadataTrackColumns.resolved(tableWidth: 300)
        for width in stride(from: 301.0, through: 2000.0, by: 1) {
            let columns = ReleaseMetadataTrackColumns.resolved(
                tableWidth: width
            )
            #expect(columns.source >= previous.source)
            #expect(columns.title >= previous.title)
            #expect(columns.artist >= previous.artist)
            previous = columns
        }
    }
}
