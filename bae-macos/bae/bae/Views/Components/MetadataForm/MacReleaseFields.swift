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
    /// Between one row's text and the next, counting the fields' own inline
    /// padding.
    static let rowSpacing = ThemeSpace.compact
    /// An empty field's width, so there is something to click into.
    static let emptyValueWidth: CGFloat = 96

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.related) {
            MacFormHeading(title: String(localized: "Release"))
            Grid(
                alignment: .leadingFirstTextBaseline,
                horizontalSpacing: Self.labelGap
                    - FieldChrome.inlineHorizontalPadding,
                verticalSpacing: Self.rowSpacing
                    - 2 * FieldChrome.inlineVerticalPadding
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
                }
                labelRows
                GridRow {
                    rowLabel(String(localized: "Country"))
                    ReleaseAreaPicker(
                        area: values.pressing.facts.area,
                        write: writer.setPressingFact
                    )
                    .accessibilityLabel(String(localized: "Country"))
                }
                barcodeRows
                GridRow {
                    rowLabel(String(localized: "Status"))
                    ReleaseStatusPicker(
                        status: values.pressing.facts.status,
                        write: writer.setPressingFact
                    )
                    .accessibilityLabel(String(localized: "Status"))
                }
                GridRow {
                    rowLabel(String(localized: "Packaging"))
                    PackagingPicker(
                        packaging: values.pressing.facts.packaging,
                        write: writer.setPressingFact
                    )
                    .accessibilityLabel(String(localized: "Packaging"))
                }
                GridRow {
                    rowLabel(String(localized: "Details"))
                    DiscogsDetailsEditor(
                        details: values.pressing.facts.discogsDetails,
                        offered: pressingOffers.discogsDetails,
                        write: writer.setPressingFact
                    )
                }
            }
        }
    }

    /// One row per label, name beside number, as core hands them: a blank row
    /// when there is none, and each row with the heading and remove button
    /// it carries, and Add on its own line under the last row.
    @ViewBuilder
    private var labelRows: some View {
        let rows = values.pressing.labels
        ForEach(Array(rows.enumerated()), id: \.offset) { index, row in
            GridRow {
                rowLabel(row.place.labelled ? String(localized: "Label") : "")
                HStack(spacing: ThemeSpace.compact) {
                    valueText(
                        row.label.name,
                        name: String(localized: "Label"),
                        monospaced: false
                    ) { name in
                        var next = rows.map(\.label)
                        next[index].name = name
                        await writer.setLabels(next)
                    }
                    valueText(
                        row.label.catalogNumber,
                        name: String(localized: "Catalog number"),
                        monospaced: true
                    ) { number in
                        var next = rows.map(\.label)
                        next[index].catalogNumber = number
                        await writer.setLabels(next)
                    }
                    if row.place.removable {
                        let named =
                            row.label.name.isEmpty
                            ? row.label.catalogNumber : row.label.name
                        Button {
                            var next = rows.map(\.label)
                            next.remove(at: index)
                            Task { await writer.setLabels(next) }
                        } label: {
                            Image(systemName: "xmark.circle.fill")
                                .foregroundStyle(.tertiary)
                        }
                        .buttonStyle(.accentPlain)
                        .help(String(localized: "Remove \(named)"))
                        .accessibilityLabel(
                            String(localized: "Remove \(named)")
                        )
                    }
                }
                .themeIcon(.small)
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
    }

    /// One row per barcode, as core hands them: a blank row when there is
    /// none, and each row with the heading and remove button it carries, and
    /// Add on its own line under the last row.
    @ViewBuilder
    private var barcodeRows: some View {
        let rows = values.pressing.barcodes
        ForEach(Array(rows.enumerated()), id: \.offset) { index, row in
            GridRow {
                rowLabel(row.place.labelled ? String(localized: "Barcode") : "")
                HStack(spacing: ThemeSpace.compact) {
                    valueText(
                        row.barcode,
                        name: String(localized: "Barcode"),
                        monospaced: true
                    ) { barcode in
                        var next = rows.map(\.barcode)
                        next[index] = barcode
                        await writer.setBarcodes(next)
                    }
                    if row.place.removable {
                        Button {
                            var next = rows.map(\.barcode)
                            next.remove(at: index)
                            Task { await writer.setBarcodes(next) }
                        } label: {
                            Image(systemName: "xmark.circle.fill")
                                .foregroundStyle(.tertiary)
                        }
                        .buttonStyle(.accentPlain)
                        .help(String(localized: "Remove \(row.barcode)"))
                        .accessibilityLabel(
                            String(localized: "Remove \(row.barcode)")
                        )
                    }
                }
                .themeIcon(.small)
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
    }

    /// A field as wide as its value, or `emptyValueWidth` when empty, when
    /// it shows the form's dash. `name` is what assistive technology calls
    /// it.
    private func valueText(
        _ text: String,
        name: String,
        monospaced: Bool,
        onCommit: @escaping @MainActor (String) async -> Void
    ) -> some View {
        MacInlineField(
            placeholder: "\u{2014}",
            value: text,
            name: name,
            monospaced: monospaced,
            font: ThemeText.body.nsFont,
            placeholderRole: .emptyMark,
            editingCommands: editingCommands,
            onCommit: onCommit
        )
        .fixedSize(horizontal: true, vertical: false)
        .frame(
            minWidth: text.isEmpty ? Self.emptyValueWidth : nil,
            alignment: .leading
        )
    }

    /// A row's label, right-aligned in its column.
    private func rowLabel(_ label: String) -> some View {
        Text(label)
            .themeText(.detail)
            .foregroundStyle(.secondary)
            .lineLimit(1)
            .frame(width: Self.labelWidth, alignment: .trailing)
    }

    /// One row: the label right-aligned in its column, then the field.
    @ViewBuilder
    private func field(
        _ field: BridgeCandidateEditField,
        label: String,
        text: String,
        monospaced: Bool = false
    ) -> some View {
        rowLabel(label)
        valueText(
            text,
            name: label,
            monospaced: monospaced
        ) { await writer.setField(field, $0) }
    }
}
