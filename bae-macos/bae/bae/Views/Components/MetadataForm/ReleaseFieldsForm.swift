import BaeKit
import SwiftUI

/// The release's own fields — album title and artist, then the six pressing
/// fields — as two grouped cards of label-left / value-right rows.
///
/// Split out of `EditMetadataForm` because the import's mapping pane edits
/// exactly these: its tracks are the slot table, so the track half of the
/// metadata form has no place there.
struct ReleaseFieldsForm: View {
    @Binding
    var form: BridgeRawReleaseEdit

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            albumGroup
            pressingGroup
        }
    }

    private var albumGroup: some View {
        groupCard(
            title: String(localized: "Album"),
            rows: [
                FieldRow(
                    label: String(localized: "Title"),
                    placeholder: String(localized: "Album title"),
                    text: $form.albumTitle,
                    width: .long,
                ),
                FieldRow(
                    label: String(localized: "Artist"),
                    hint: String(localized: "comma-separated"),
                    placeholder: String(localized: "Album artist"),
                    text: $form.albumArtistText,
                    width: .long,
                ),
            ],
        )
    }

    private var pressingGroup: some View {
        groupCard(
            title: String(localized: "Release pressing"),
            rows: [
                FieldRow(
                    label: String(localized: "Year"),
                    placeholder: String(localized: "Year"),
                    text: $form.pressing.year,
                    width: .short,
                    monospaced: true,
                ),
                FieldRow(
                    label: String(localized: "Format"),
                    placeholder: String(localized: "Format"),
                    text: $form.pressing.format,
                    width: .short,
                ),
                FieldRow(
                    label: String(localized: "Label"),
                    placeholder: String(localized: "Label"),
                    text: $form.pressing.label,
                    width: .long,
                ),
                FieldRow(
                    label: String(localized: "Catalog number"),
                    placeholder: String(localized: "Catalog number"),
                    text: $form.pressing.catalogNumber,
                    width: .medium,
                ),
                FieldRow(
                    label: String(localized: "Country"),
                    placeholder: String(localized: "Country"),
                    text: $form.pressing.country,
                    width: .short,
                ),
                FieldRow(
                    label: String(localized: "Barcode"),
                    placeholder: String(localized: "Barcode"),
                    text: $form.pressing.barcode,
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
            MetadataField(
                placeholder: row.placeholder,
                text: row.text,
                monospaced: row.monospaced,
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
