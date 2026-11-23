import BaeKit
import SwiftUI

/// The editable release facts under a ruled Release header.
struct MacReleaseFields: View {
    let values: BridgeRawReleaseEdit
    /// The media and details the pressing's Add menus offer, as core's read
    /// hands them beside `values`.
    let pressingOffers: BridgePressingOffers
    let writer: ReleaseFieldWriter
    let editingCommands: EditingCommitCommands

    static let labelWidth: CGFloat = 64
    static let labelGap = ThemeSpace.group
    /// Between one row and the next.
    static let rowSpacing = ThemeSpace.compact
    /// The least height of a row: the inline fields are 22 points tall.
    static let rowHeight: CGFloat = 24
    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.related) {
            MacFormHeading(title: String(localized: "Release"))
            Grid(
                alignment: .topLeading,
                horizontalSpacing: Self.labelGap,
                verticalSpacing: Self.rowSpacing
            ) {
                GridRow {
                    field(
                        .pressingYear,
                        label: String(localized: "Year"),
                        text: values.pressing.year
                    )
                }
                GridRow {
                    rowLabel(localizedCoreString("core.release.media"))
                    MediaCountsEditor(
                        media: values.pressing.facts.media,
                        offered: pressingOffers.media,
                        write: writer.setPressingFact
                    )
                    .valueCell()
                }
                labelRows
                GridRow {
                    rowLabel(String(localized: "Country"))
                    ReleaseAreaPicker(
                        area: values.pressing.facts.area,
                        write: writer.setPressingFact
                    )
                    .accessibilityLabel(String(localized: "Country"))
                    .valueCell()
                }
                barcodeRows
                GridRow {
                    rowLabel(String(localized: "Status"))
                    OptionalFactPicker.status(
                        values.pressing.facts.status,
                        write: writer.setPressingFact
                    )
                    .accessibilityLabel(String(localized: "Status"))
                    .valueCell()
                }
                GridRow {
                    rowLabel(String(localized: "Packaging"))
                    OptionalFactPicker.packaging(
                        values.pressing.facts.packaging,
                        write: writer.setPressingFact
                    )
                    .accessibilityLabel(String(localized: "Packaging"))
                    .valueCell()
                }
                GridRow {
                    rowLabel(String(localized: "Details"))
                    DiscogsDetailsEditor(
                        details: values.pressing.facts.discogsDetails,
                        offered: pressingOffers.discogsDetails,
                        write: writer.setPressingFact
                    )
                    .valueCell()
                }
            }
        }
    }

    /// One row per label, name beside number, each with its remove button
    /// and the heading on the first. Add is on its own line under the last
    /// row, or beside the heading while there is none.
    @ViewBuilder
    private var labelRows: some View {
        let rows = values.pressing.labels
        if rows.isEmpty {
            GridRow {
                rowLabel(String(localized: "Label"))
                addLabel(rows: rows)
            }
        }
        ForEach(Array(rows.enumerated()), id: \.offset) { index, row in
            GridRow {
                rowLabel(row.place.labelled ? String(localized: "Label") : "")
                HStack(spacing: ThemeSpace.compact) {
                    valueText(
                        row.label.name,
                        name: String(localized: "Label")
                    ) { name in
                        var next = rows.map(\.label)
                        next[index].name = name
                        await writer.setLabels(next)
                    }
                    valueText(
                        row.label.catalogNumber,
                        name: String(localized: "Catalog number")
                    ) { number in
                        var next = rows.map(\.label)
                        next[index].catalogNumber = number
                        await writer.setLabels(next)
                    }
                    let named =
                        row.label.name.isEmpty
                        ? row.label.catalogNumber : row.label.name
                    MacIconButton.remove(named) {
                        var next = rows.map(\.label)
                        next.remove(at: index)
                        Task { await writer.setLabels(next) }
                    }
                }
            }
            if row.place.addsRow {
                GridRow {
                    rowLabel("")
                    addLabel(rows: rows)
                }
            }
        }
    }

    private func addLabel(rows: [BridgeLabelRow]) -> some View {
        Button {
            let blank = BridgeRawLabelEdit(name: "", catalogNumber: "")
            Task { await writer.setLabels(rows.map(\.label) + [blank]) }
        } label: {
            AddEntryLabel(words: String(localized: "Add label"))
        }
        .buttonStyle(.accentPlain)
        .valueCell()
    }

    /// One row per barcode, each with its remove button and the heading on
    /// the first. Add is on its own line under the last row, or beside the
    /// heading while there is none.
    @ViewBuilder
    private var barcodeRows: some View {
        let rows = values.pressing.barcodes
        if rows.isEmpty {
            GridRow {
                rowLabel(String(localized: "Barcode"))
                addBarcode(rows: rows)
            }
        }
        ForEach(Array(rows.enumerated()), id: \.offset) { index, row in
            GridRow {
                rowLabel(row.place.labelled ? String(localized: "Barcode") : "")
                HStack(spacing: ThemeSpace.compact) {
                    valueText(
                        row.barcode,
                        name: String(localized: "Barcode")
                    ) { barcode in
                        var next = rows.map(\.barcode)
                        next[index] = barcode
                        await writer.setBarcodes(next)
                    }
                    MacIconButton.remove(row.barcode) {
                        var next = rows.map(\.barcode)
                        next.remove(at: index)
                        Task { await writer.setBarcodes(next) }
                    }
                }
            }
            if row.place.addsRow {
                GridRow {
                    rowLabel("")
                    addBarcode(rows: rows)
                }
            }
        }
    }

    private func addBarcode(rows: [BridgeBarcodeRow]) -> some View {
        Button {
            Task { await writer.setBarcodes(rows.map(\.barcode) + [""]) }
        } label: {
            AddEntryLabel(words: String(localized: "Add barcode"))
        }
        .buttonStyle(.accentPlain)
        .valueCell()
    }

    /// A field as wide as its value, or as wide as an empty field is, showing
    /// the form's dash. `name` is what assistive technology calls it.
    private func valueText(
        _ text: String,
        name: String,
        onCommit: @escaping @MainActor (String) async -> Void
    ) -> some View {
        MacInlineField(
            placeholder: FactMenuLabel.emptyMark,
            value: text,
            name: name,
            fitsValue: true,
            font: ThemeText.body.nsFont,
            placeholderRole: .emptyMark,
            editingCommands: editingCommands,
            onCommit: onCommit
        )
        .valueCell()
    }

    /// A row's label, right-aligned in its column.
    private func rowLabel(_ label: String) -> some View {
        Text(label)
            .themeText(.detail)
            .foregroundStyle(Theme.macTextSecondary)
            .lineLimit(1)
            .frame(width: Self.labelWidth, alignment: .trailing)
    }

    /// One row: the label right-aligned in its column, then the field.
    @ViewBuilder
    private func field(
        _ field: BridgeCandidateEditField,
        label: String,
        text: String
    ) -> some View {
        rowLabel(label)
        valueText(
            text,
            name: label
        ) { await writer.setField(field, $0) }
    }
}

extension View {
    /// A release field's value, in a cell at least as tall as a row of the
    /// grid, leading-aligned.
    fileprivate func valueCell() -> some View {
        frame(minHeight: MacReleaseFields.rowHeight, alignment: .leading)
    }
}
