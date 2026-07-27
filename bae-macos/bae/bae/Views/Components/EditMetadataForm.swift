import BaeKit
import SwiftUI

/// Pure form over a raw edit-metadata payload (`BridgeRawReleaseEdit`).
/// Two-way binding on the raw text the user types; no async state, no
/// save/cancel buttons — those belong to the surrounding surface. The
/// form does no shaping or validation: it collects raw values and bae-core
/// turns them into a wire edit via `shapeReleaseEdit`.
///
/// Used by:
///
/// - `EditMetadataSheet` — wraps with a header (Cancel) and footer
///   (Reset, Save) for the post-commit "Edit metadata..." sheet.
/// - `ImportConfirmationView` — embeds inline above the import-specific
///   chrome (Import button, library banner). The user's edits are
///   committed alongside the import command as the metadata overlay.
///
/// Layout: Album and Release-pressing fields are label-left / value-right
/// rows inside grouped inset cards; tracks are one table with a single
/// header row and compact editable cells, so the per-track labels and the
/// "blank = album artist" hint appear once instead of repeating down every
/// row. The view does not scroll — the surrounding surface owns scrolling.
struct EditMetadataForm: View {
    @Binding
    var form: BridgeRawReleaseEdit

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            albumGroup
            pressingGroup
            tracksGroup
        }
    }

    // MARK: - Album

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

    // MARK: - Release pressing

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

    // MARK: - Grouped card

    /// A titled inset card of label-left / value-right rows separated by
    /// hairlines — the macOS System-Settings group idiom.
    private func groupCard(title: String, rows: [FieldRow]) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            sectionHeader(title: title, trailing: nil)
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
            .background(Theme.surface)
            .clipShape(RoundedRectangle(cornerRadius: 10))
            .overlay {
                RoundedRectangle(cornerRadius: 10)
                    .strokeBorder(.white.opacity(0.07), lineWidth: 1)
            }
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

    // MARK: - Section header

    private func sectionHeader(
        title: String,
        trailing: String?
    ) -> some View {
        HStack(alignment: .firstTextBaseline) {
            eyebrow(Text(verbatim: title), size: 11)
            Spacer()
            if let trailing {
                Text(trailing)
                    .font(.system(size: 11.5))
                    .monospacedDigit()
                    .foregroundStyle(.tertiary)
            }
        }
        .padding(.horizontal, 2)
    }

    /// Section headers render at 11pt; the denser track-table column
    /// headers pass `size: 10`.
    private func eyebrow(_ text: Text, size: CGFloat = 10) -> some View {
        text
            .font(.system(size: size, weight: .bold))
            .textCase(.uppercase)
            .tracking(1)
            .foregroundStyle(.tertiary)
    }
}

// MARK: - Tracks

extension EditMetadataForm {
    fileprivate var tracksGroup: some View {
        VStack(alignment: .leading, spacing: 8) {
            sectionHeader(
                title: String(localized: "Tracks"),
                trailing: trackCountLabel
            )
            VStack(spacing: 0) {
                trackHeaderRow
                if form.tracks.isEmpty {
                    Text("No tracks")
                        .font(.callout)
                        .foregroundStyle(.secondary)
                        .frame(maxWidth: .infinity)
                        .padding(.vertical, 16)
                }
                else {
                    ForEach(
                        Array(form.tracks.enumerated()),
                        id: \.element.id
                    ) { index, _ in
                        trackRow(
                            index: index,
                            track: $form.tracks[index],
                            isLast: index == form.tracks.count - 1,
                        )
                    }
                }
            }
            .background(Theme.surface)
            .clipShape(RoundedRectangle(cornerRadius: 10))
            .overlay {
                RoundedRectangle(cornerRadius: 10)
                    .strokeBorder(.white.opacity(0.07), lineWidth: 1)
            }
        }
    }

    private var trackCountLabel: String {
        String(localized: "\(form.tracks.count) tracks")
    }

    private var trackHeaderRow: some View {
        HStack(spacing: 10) {
            eyebrow(Text(verbatim: "#"))
                .frame(width: trackOrdinalWidth)
            eyebrow(Text("Title"))
                .frame(maxWidth: .infinity, alignment: .leading)
            HStack(spacing: 4) {
                eyebrow(Text("Artist"))
                Text("· blank = album artist")
                    .font(.system(size: 10))
                    .foregroundStyle(.quaternary)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            eyebrow(Text("Disc"))
                .frame(width: trackNumberWidth)
            eyebrow(Text("Track"))
                .frame(width: trackNumberWidth)
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 8)
        .background(Theme.surfaceElevated)
        .overlay(alignment: .bottom) {
            Rectangle()
                .fill(.white.opacity(0.13))
                .frame(height: 1)
        }
    }

    private func trackRow(
        index: Int,
        track: Binding<BridgeRawTrackEdit>,
        isLast: Bool,
    ) -> some View {
        HStack(spacing: 10) {
            Text(verbatim: (index + 1).formatted())
                .font(.system(size: 12))
                .monospacedDigit()
                .foregroundStyle(.tertiary)
                .frame(width: trackOrdinalWidth)
            MetadataField(
                placeholder: String(localized: "Title"),
                text: track.title,
                boxed: false,
            )
            .frame(maxWidth: .infinity)
            MetadataField(
                placeholder: trackArtistPlaceholder,
                text: track.artistText,
                boxed: false,
            )
            .frame(maxWidth: .infinity)
            TrackSideCell(value: track.side)
                .frame(width: trackNumberWidth)
            TrackNumberCell(value: track.trackNumber)
                .frame(width: trackNumberWidth)
        }
        .padding(.horizontal, 14)
        .frame(minHeight: 38)
        .background(
            index.isMultiple(of: 2) ? Color.clear : .white.opacity(0.02)
        )
        .overlay(alignment: .bottom) {
            // Kept in the tree on every row and hidden on the last, so a
            // row's intrinsic size never changes across the table.
            Rectangle()
                .fill(.white.opacity(0.07))
                .frame(height: 1)
                .opacity(isLast ? 0 : 1)
                .allowsHitTesting(false)
        }
    }

    /// Empty track-artist fields inherit the album artist; surfacing it as
    /// the placeholder shows what a blank row will resolve to.
    private var trackArtistPlaceholder: String {
        form.albumArtistText.isEmpty
            ? String(localized: "Artist") : form.albumArtistText
    }

    private var trackOrdinalWidth: CGFloat { 34 }
    private var trackNumberWidth: CGFloat { 58 }
}

#if DEBUG
    #Preview("Edit Metadata Form") {
        @Previewable
        @State
        var form = PreviewData.editMetadataSeed(trackCount: 13)
        ScrollView {
            EditMetadataForm(form: $form)
                .padding(20)
        }
        .frame(width: 640, height: 720)
        .background(Theme.background)
        .preferredColorScheme(.dark)
    }
#endif
