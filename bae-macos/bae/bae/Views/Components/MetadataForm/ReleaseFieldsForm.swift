import BaeKit
import SwiftUI

/// Where one field's typed value goes.
///
/// The library's release editor holds its form in memory and saves it whole,
/// so its fields write into a binding. The import pane holds nothing: each
/// field is a row under the candidate, and typing in one writes that row. Both
/// hand this the same eight fields.
struct ReleaseFieldWriter {
    let setField: (BridgeCandidateEditField, String) -> Void

    /// Write into a form held in memory.
    static func binding(_ form: Binding<BridgeRawReleaseEdit>) -> Self {
        Self { field, value in
            switch field {
            case .albumTitle: form.wrappedValue.albumTitle = value
            case .albumArtistText: form.wrappedValue.albumArtistText = value
            case .year: form.wrappedValue.pressing.year = value
            case .format: form.wrappedValue.pressing.format = value
            case .label: form.wrappedValue.pressing.label = value
            case .catalogNumber:
                form.wrappedValue.pressing.catalogNumber = value
            case .country: form.wrappedValue.pressing.country = value
            case .barcode: form.wrappedValue.pressing.barcode = value
            }
        }
    }
}

/// The album and pressing fields of a release, as one form.
///
/// It reads values and reports edits; where those values live is the caller's.
struct ReleaseFieldsForm: View {
    let values: BridgeRawReleaseEdit
    let writer: ReleaseFieldWriter

    init(values: BridgeRawReleaseEdit, writer: ReleaseFieldWriter) {
        self.values = values
        self.writer = writer
    }

    /// A form over a value held in memory — the library's release editor.
    init(form: Binding<BridgeRawReleaseEdit>) {
        values = form.wrappedValue
        writer = .binding(form)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            albumGroup
            pressingGroup
        }
    }

    private func row(
        _ field: BridgeCandidateEditField,
        label: String,
        hint: String? = nil,
        placeholder: String,
        text: String,
        width: FieldWidth,
        monospaced: Bool = false
    ) -> FieldRow {
        FieldRow(
            label: label,
            hint: hint,
            placeholder: placeholder,
            text: text,
            onCommit: { writer.setField(field, $0) },
            width: width,
            monospaced: monospaced,
        )
    }

    private var albumGroup: some View {
        groupCard(
            title: String(localized: "Album"),
            rows: [
                row(
                    .albumTitle,
                    label: String(localized: "Title"),
                    placeholder: String(localized: "Album title"),
                    text: values.albumTitle,
                    width: .long,
                ),
                row(
                    .albumArtistText,
                    label: String(localized: "Artist"),
                    hint: String(localized: "comma-separated"),
                    placeholder: String(localized: "Album artist"),
                    text: values.albumArtistText,
                    width: .long,
                ),
            ],
        )
    }

    private var pressingGroup: some View {
        groupCard(
            title: String(localized: "Release pressing"),
            rows: [
                row(
                    .year,
                    label: String(localized: "Year"),
                    placeholder: String(localized: "Year"),
                    text: values.pressing.year,
                    width: .short,
                    monospaced: true,
                ),
                row(
                    .format,
                    label: String(localized: "Format"),
                    placeholder: String(localized: "Format"),
                    text: values.pressing.format,
                    width: .short,
                ),
                row(
                    .label,
                    label: String(localized: "Label"),
                    placeholder: String(localized: "Label"),
                    text: values.pressing.label,
                    width: .long,
                ),
                row(
                    .country,
                    label: String(localized: "Country"),
                    placeholder: String(localized: "Country"),
                    text: values.pressing.country,
                    width: .short,
                ),
                row(
                    .catalogNumber,
                    label: String(localized: "Catalog number"),
                    placeholder: String(localized: "Catalog number"),
                    text: values.pressing.catalogNumber,
                    width: .medium,
                ),
                row(
                    .barcode,
                    label: String(localized: "Barcode"),
                    placeholder: String(localized: "Barcode"),
                    text: values.pressing.barcode,
                    width: .medium,
                    monospaced: true,
                ),
            ],
        )
    }

    /// A titled inset card of label-left / value-right rows separated by
    /// hairlines.
    private func groupCard(title: String, rows: [FieldRow]) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            FormSectionHeader(title: title)
            VStack(spacing: 0) {
                ForEach(Array(rows.enumerated()), id: \.element.label) {
                    index,
                    row in
                    fieldRow(row)
                    if index < rows.count - 1 {
                        Rectangle()
                            .fill(.white.opacity(0.07))
                            .frame(height: 1)
                    }
                }
            }
            .formGroupCard()
        }
    }

    private func fieldRow(_ row: FieldRow) -> some View {
        HStack(spacing: 16) {
            VStack(alignment: .leading, spacing: 1) {
                Text(row.label)
                    .font(.system(size: 13))
                if let hint = row.hint {
                    Text(hint)
                        .font(.system(size: 10.5))
                        .foregroundStyle(.quaternary)
                }
            }
            .frame(width: 150, alignment: .leading)
            CommittedTextField(
                placeholder: row.placeholder,
                value: row.text,
                monospaced: row.monospaced,
                onCommit: row.onCommit,
            )
            .frame(maxWidth: row.width.maxWidth)
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 9)
    }
}

#if DEBUG
    #Preview("Release fields") {
        @Previewable
        @State
        var form = PreviewData.editMetadataSeed(trackCount: 3)
        ScrollView {
            ReleaseFieldsForm(form: $form)
                .padding(20)
        }
        .frame(width: 640, height: 520)
        .background(Theme.background)
        .preferredColorScheme(.dark)
    }
#endif
