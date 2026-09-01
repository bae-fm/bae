import Foundation
import Testing

@testable import bae

/// The mapping table's column widths, against the width the table has to lay
/// them out in.
///
/// The one that matters is that each section adds up: Tracks uses its five
/// columns plus the action slot, and Files uses Name plus Size. Neither may
/// reserve an invisible slice past the pane's right edge.
struct ImportMappingColumnsTests {
    /// Every column, the action slot, the gaps between them and the row's two
    /// leading edges, as the row lays them out.
    private func rowWidth(_ columns: ImportMappingColumns) -> CGFloat {
        columns.tracks.title + columns.tracks.artist + columns.tracks.source
            + ImportMappingColumns.position + ImportMappingColumns.length
            + ImportMappingColumns.action
            + ImportMappingColumns.spacing * 5
            + ImportMappingColumns.rowPadding * 2
    }

    @Test(
        "the Files columns occupy the whole inner table",
        arguments: [
            0,
            ImportMappingColumns.minimumTableWidth,
            ImportMappingColumns.idealTableWidth,
            1200,
        ] as [CGFloat]
    )
    func filesColumnsUseTheInnerWidth(width: CGFloat) {
        let columns = ImportMappingColumns.resolved(tableWidth: width)

        // Name, Size, and the trailing reserve that keeps Size's right edge
        // level with the tracks section's Length column.
        #expect(
            columns.files.name + ImportMappingColumns.spacing
                + columns.files.size + ImportMappingColumns.spacing
                + ImportMappingColumns.action
                == max(width, ImportMappingColumns.minimumTableWidth)
                - ImportMappingColumns.rowPadding * 2
        )
    }

    @Test(
        arguments: [
            ImportMappingColumns.minimumTableWidth,
            660, 700, 733.5, 800, 900,
            ImportMappingColumns.idealTableWidth,
            1200, 1600, 2400,
        ] as [CGFloat]
    )
    func theRowIsExactlyTheTableWide(width: CGFloat) {
        let columns = ImportMappingColumns.resolved(tableWidth: width)

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
            ImportMappingColumns.minimumTableWidth / 4,
            ImportMappingColumns.minimumTableWidth / 2,
            ImportMappingColumns.minimumTableWidth - 1,
        ] as [CGFloat]
    )
    func aTableTooNarrowForItsColumnsIsLaidOutAtItsMinimum(width: CGFloat) {
        let columns = ImportMappingColumns.resolved(tableWidth: width)

        let atMinimum = ImportMappingColumns.resolved(
            tableWidth: ImportMappingColumns.minimumTableWidth
        )

        #expect(rowWidth(columns) == ImportMappingColumns.minimumTableWidth)
        #expect(columns.tracks.source == atMinimum.tracks.source)
        #expect(columns.tracks.title == atMinimum.tracks.title)
        #expect(columns.tracks.artist == atMinimum.tracks.artist)
    }

    /// Wide enough and every Tracks column has the width it asks for, with the
    /// surplus going to Source and nowhere else.
    @Test(arguments: [0, 1, 200, 900] as [CGFloat])
    func theSurplusAboveTheIdealWidthIsAllTheSource(surplus: CGFloat) {
        let columns = ImportMappingColumns.resolved(
            tableWidth: ImportMappingColumns.idealTableWidth + surplus
        )

        #expect(columns.tracks.title == 220)
        #expect(columns.tracks.artist == 180)
        #expect(columns.tracks.source == 260 + surplus)
    }

    /// Narrowing takes from all three at once. A column that kept its width
    /// while its neighbour collapsed would be the same bug in miniature: the
    /// row still fits, and one cell has stopped saying anything.
    @Test
    func narrowingTakesFromEveryColumnThatHasGive() {
        let wide = ImportMappingColumns.resolved(
            tableWidth: ImportMappingColumns.idealTableWidth
        )
        let middle = ImportMappingColumns.resolved(
            tableWidth: (ImportMappingColumns.idealTableWidth
                + ImportMappingColumns.minimumTableWidth) / 2
        )
        let narrow = ImportMappingColumns.resolved(
            tableWidth: ImportMappingColumns.minimumTableWidth
        )

        #expect(middle.tracks.source < wide.tracks.source)
        #expect(middle.tracks.title < wide.tracks.title)
        #expect(middle.tracks.artist < wide.tracks.artist)
        #expect(narrow.tracks.source < middle.tracks.source)
        #expect(narrow.tracks.title < middle.tracks.title)
        #expect(narrow.tracks.artist < middle.tracks.artist)
    }

    /// Widening never narrows a column and narrowing never widens one, at every
    /// width in between — the columns follow the pane rather than jumping about
    /// as it is dragged.
    @Test
    func everyColumnFollowsTheTableWidthOneWay() {
        var previous = ImportMappingColumns.resolved(tableWidth: 300)
        for width in stride(from: 301.0, through: 2000.0, by: 1) {
            let columns = ImportMappingColumns.resolved(tableWidth: width)
            #expect(columns.tracks.source >= previous.tracks.source)
            #expect(columns.tracks.title >= previous.tracks.title)
            #expect(columns.tracks.artist >= previous.tracks.artist)
            previous = columns
        }
    }
}
