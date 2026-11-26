import BaeKit
import SwiftUI

/// An identifier kind the band found, which heads a column of the pressing
/// table; a dot in the column marks each pressing the kind names.
enum PressingKind: Hashable, CaseIterable {
    case discId
    case barcode
    case catalog

    /// The kind's name, as the band's chip and the column's heading say it.
    var title: String {
        switch self {
        case .discId:
            SignalBadgeStyle.label(for: SignalBadgeStyle.Signal.discId)
        case .barcode:
            SignalBadgeStyle.label(for: SignalBadgeStyle.Signal.barcode)
        case .catalog: String(localized: "Catalog #")
        }
    }

    /// Whether the run's own text agrees with the pressing through this
    /// kind.
    func names(_ agreements: BridgeAgreements) -> Bool {
        switch self {
        case .discId: agreements.discId
        case .barcode: agreements.barcode
        case .catalog: agreements.catalog
        }
    }
}

extension PressingKind {
    /// The kind core says found releases.
    init(_ kind: BridgeIdentifierKind) {
        switch kind {
        case .discId: self = .discId
        case .barcode: self = .barcode
        case .catalog: self = .catalog
        }
    }
}

/// The width of the dot that marks a pressing a kind names.
let pressingDotSize: CGFloat = 6

/// A column of the table that takes the width of its widest cell.
enum PressingColumn: Hashable {
    case year
    case label
    case catalog
    case country
    case format
    case kind(PressingKind)

    /// The columns before the details, which take the spare width.
    static let beforeDetails: [PressingColumn] = [
        .year, .label, .catalog, .country, .format,
    ]

    /// The least width the column takes, as wide as its widest cell
    /// otherwise.
    fileprivate var leastWidth: CGFloat {
        switch self {
        // The board's 48 point column holds the row's 8 point inset.
        case .year: 40
        case .label: 200
        case .kind: pressingDotSize
        case .catalog, .country, .format: 0
        }
    }
}

/// What every row of a table shares: the identifier kinds it has columns for
/// and how wide each column is.
struct PressingTableColumns: Equatable {
    var kinds: [PressingKind] = []
    /// The widest each column's cells are drawn, as the rows last reported.
    var measured: [PressingColumn: CGFloat] = [:]

    /// The column's width; `nil` until a row has reported its cells.
    func width(of column: PressingColumn) -> CGFloat? {
        measured[column].map { max($0, column.leastWidth) }
    }
}

extension EnvironmentValues {
    @Entry
    var pressingTable = PressingTableColumns()
}

private struct PressingColumnWidths: PreferenceKey {
    static let defaultValue: [PressingColumn: CGFloat] = [:]

    static func reduce(
        value: inout [PressingColumn: CGFloat],
        nextValue: () -> [PressingColumn: CGFloat]
    ) {
        value.merge(nextValue(), uniquingKeysWith: max)
    }
}

/// The pressings of Find online's results as one table: every row of every
/// album shares one grid, so each column starts at one x on every row. The
/// identifier kinds' names head their columns once, above the first album.
struct MacPressingTable<Content: View>: View {
    let kinds: [PressingKind]
    @ViewBuilder
    let content: Content

    @State
    private var measured: [PressingColumn: CGFloat] = [:]

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.line) {
            if !kinds.isEmpty {
                PressingTableRow { column in
                    if case .kind(let kind) = column {
                        PressingCell(column: column, alignment: .center) {
                            Text(kind.title)
                                .themeText(.detail)
                                .foregroundStyle(Theme.textTertiary)
                        }
                    }
                    else {
                        Color.clear
                            .frame(
                                width: pressingTable.width(of: column),
                                height: 0
                            )
                    }
                } details: {
                    Color.clear.frame(height: 0)
                } chevron: {
                    Color.clear.frame(height: 0)
                }
                .padding(.horizontal, MacPressingRow.horizontalPadding)
            }
            content
        }
        .environment(\.pressingTable, pressingTable)
        .onPreferenceChange(PressingColumnWidths.self) { measured = $0 }
    }

    private var pressingTable: PressingTableColumns {
        PressingTableColumns(kinds: kinds, measured: measured)
    }
}

/// One row of the table's grid: the columns before the details, the details,
/// one column for each identifier kind, and the chevron. `cell` draws the
/// column it is asked for.
struct PressingTableRow<Cell: View, Details: View, Chevron: View>: View {
    /// The least width of the details column.
    private static var detailsLeastWidth: CGFloat { 150 }
    /// The chevron's column: its 11 point frame and a section's space before
    /// it, less the row's 8 point inset that the board's column holds.
    private static var chevronColumnWidth: CGFloat {
        11 + ThemeSpace.section - MacPressingRow.horizontalPadding
    }

    @Environment(\.pressingTable)
    private var table

    @ViewBuilder
    let cell: (PressingColumn) -> Cell
    @ViewBuilder
    let details: Details
    @ViewBuilder
    let chevron: Chevron

    var body: some View {
        HStack(spacing: ThemeSpace.edge) {
            ForEach(PressingColumn.beforeDetails, id: \.self) { column in
                cell(column)
            }
            details
                .frame(
                    minWidth: Self.detailsLeastWidth,
                    maxWidth: .infinity,
                    alignment: .leading
                )
            ForEach(table.kinds, id: \.self) { kind in
                cell(.kind(kind))
            }
            chevron
                .frame(width: Self.chevronColumnWidth, alignment: .trailing)
        }
    }
}

/// One cell of a column: drawn at the column's width, and reporting the width
/// it needs so the column takes the widest cell's.
struct PressingCell<Content: View>: View {
    let column: PressingColumn
    var alignment: Alignment = .leading
    @ViewBuilder
    let content: Content

    @Environment(\.pressingTable)
    private var table

    var body: some View {
        content
            .cutLine(.prose)
            .frame(width: table.width(of: column), alignment: alignment)
            .background(alignment: .leading) {
                content
                    .lineLimit(1)
                    .fixedSize()
                    .background(
                        GeometryReader { proxy in
                            Color.clear.preference(
                                key: PressingColumnWidths.self,
                                value: [column: proxy.size.width]
                            )
                        }
                    )
                    .opacity(0)
                    .accessibilityHidden(true)
            }
    }
}
