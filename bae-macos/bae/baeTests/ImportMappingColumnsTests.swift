import Foundation
import Testing

@testable import bae

/// The mapping table's column widths, against the width the table has to lay
/// them out in.
///
/// The one that matters is that the tracks section adds up: five columns,
/// never reserving an invisible slice past the pane's right edge. Files rows
/// are not columnar.
struct ImportMappingColumnsTests {
    /// Every column, the gaps between them and the row's two leading edges, as
    /// the row lays them out.
    private func rowWidth(_ columns: ReleaseMetadataTrackColumns) -> CGFloat {
        columns.title + columns.artist + columns.source
            + ReleaseMetadataTrackColumns.track
            + ReleaseMetadataTrackColumns.length
            + ReleaseMetadataTrackColumns.spacing * 4
    }

    @Test(
        arguments: [
            ReleaseMetadataTrackColumns.minimumTableWidth,
            700, 733.5, 800, 900,
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

    /// Wide enough that no column sits at its floor, Source, Title and Artist
    /// share the width the fixed columns leave in the ratio 1.2 : 1 : 1, as
    /// the board's grid does.
    @Test(arguments: [900, 1200, 2400] as [CGFloat])
    func theColumnsShareTheRestInTheBoardsRatio(width: CGFloat) {
        let columns = ReleaseMetadataTrackColumns.resolved(tableWidth: width)

        #expect(abs(columns.source - columns.title * 1.2) < 0.001)
        #expect(abs(columns.artist - columns.title) < 0.001)
    }

    /// At the board's 820pt column the columns are 240, 200 and 200.
    @Test
    func theBoardsEightHundredTwentyPointTable() {
        let columns = ReleaseMetadataTrackColumns.resolved(tableWidth: 820)

        #expect(abs(columns.source - 240) < 0.001)
        #expect(abs(columns.title - 200) < 0.001)
        #expect(abs(columns.artist - 200) < 0.001)
    }

    /// A column that would fall under its floor keeps the floor, and the
    /// others share what is left.
    @Test
    func aColumnUnderItsFloorKeepsTheFloor() {
        let columns = ReleaseMetadataTrackColumns.resolved(tableWidth: 600)

        #expect(columns.artist >= 100)
        #expect(columns.title >= 120)
        #expect(columns.source >= 160)
        #expect(rowWidth(columns) == 600)
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
