import Foundation
import Testing

@testable import bae

/// The mapping table's column widths, against the width the table has to lay
/// them out in.
///
/// The one that matters is that they add up: seven columns, six gaps and two
/// leading edges came to a fixed 1042pt whatever the pane was, so a pane at any
/// ordinary window size had the length and the row's actions past its right
/// edge with no way to reach them.
struct ImportMappingColumnsTests {
    /// Every column, the gaps between them and the row's two leading edges, as
    /// the row lays them out.
    private func rowWidth(_ columns: ImportMappingColumns) -> CGFloat {
        columns.source + columns.role + columns.title + columns.artist
            + ImportMappingColumns.position + ImportMappingColumns.length
            + ImportMappingColumns.actions
            + ImportMappingColumns.spacing * 6
            + ImportMappingColumns.rowPadding * 2
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
    @Test(arguments: [0, 320, 600, ImportMappingColumns.minimumTableWidth - 1])
    func aTableTooNarrowForItsColumnsIsLaidOutAtItsMinimum(width: CGFloat) {
        let columns = ImportMappingColumns.resolved(tableWidth: width)

        let atMinimum = ImportMappingColumns.resolved(
            tableWidth: ImportMappingColumns.minimumTableWidth
        )

        #expect(rowWidth(columns) == ImportMappingColumns.minimumTableWidth)
        #expect(columns.source == atMinimum.source)
        #expect(columns.role == atMinimum.role)
        #expect(columns.title == atMinimum.title)
        #expect(columns.artist == atMinimum.artist)
    }

    /// Wide enough and every column has the width it asks for, with the surplus
    /// going to the file names and nowhere else — the layout the table has
    /// always drawn at a wide window, unchanged.
    @Test(arguments: [0, 1, 200, 900] as [CGFloat])
    func theSurplusAboveTheIdealWidthIsAllTheSource(surplus: CGFloat) {
        let columns = ImportMappingColumns.resolved(
            tableWidth: ImportMappingColumns.idealTableWidth + surplus
        )

        #expect(columns.role == 118)
        #expect(columns.title == 220)
        #expect(columns.artist == 180)
        #expect(columns.source == 240 + surplus)
    }

    /// Narrowing takes from all four at once. A column that kept its width
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

        #expect(middle.source < wide.source)
        #expect(middle.role < wide.role)
        #expect(middle.title < wide.title)
        #expect(middle.artist < wide.artist)
        #expect(narrow.source < middle.source)
        #expect(narrow.role < middle.role)
        #expect(narrow.title < middle.title)
        #expect(narrow.artist < middle.artist)
    }

    /// Widening never narrows a column and narrowing never widens one, at every
    /// width in between — the columns follow the pane rather than jumping about
    /// as it is dragged.
    @Test
    func everyColumnFollowsTheTableWidthOneWay() {
        var previous = ImportMappingColumns.resolved(tableWidth: 300)
        for width in stride(from: 301.0, through: 2000.0, by: 1) {
            let columns = ImportMappingColumns.resolved(tableWidth: width)
            #expect(columns.source >= previous.source)
            #expect(columns.role >= previous.role)
            #expect(columns.title >= previous.title)
            #expect(columns.artist >= previous.artist)
            previous = columns
        }
    }
}
