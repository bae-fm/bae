import SwiftUI

// MARK: - EditMetadataForm

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
    /// Greys out and locks the pressing fields. The import pane sets this when
    /// the Metadata-only choice is active — those fields stay blank because the
    /// user isn't claiming a specific pressing. Off everywhere else (the
    /// post-commit edit sheet always edits the pressing).
    var pressingFieldsDisabled: Bool = false

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            albumGroup
            pressingGroup
                .disabled(pressingFieldsDisabled)
                .opacity(pressingFieldsDisabled ? 0.55 : 1)
            tracksGroup
        }
    }

    // MARK: - Album

    private var albumGroup: some View {
        groupCard(
            title: "Album",
            rows: [
                FieldRow(
                    label: "Title",
                    placeholder: "Album title",
                    text: $form.albumTitle,
                    width: .long,
                ),
                FieldRow(
                    label: "Artist",
                    hint: "comma-separated",
                    placeholder: "Album artist",
                    text: $form.albumArtistText,
                    width: .long,
                ),
            ],
        )
    }

    // MARK: - Release pressing

    private var pressingGroup: some View {
        groupCard(
            title: "Release pressing",
            rows: [
                FieldRow(
                    label: "Year",
                    placeholder: "Year",
                    text: $form.pressing.year,
                    width: .short,
                    monospaced: true,
                ),
                FieldRow(
                    label: "Format",
                    placeholder: "Format",
                    text: $form.pressing.format,
                    width: .short,
                ),
                FieldRow(
                    label: "Label",
                    placeholder: "Label",
                    text: $form.pressing.label,
                    width: .long,
                ),
                FieldRow(
                    label: "Catalog number",
                    placeholder: "Catalog number",
                    text: $form.pressing.catalogNumber,
                    width: .medium,
                ),
                FieldRow(
                    label: "Country",
                    placeholder: "Country",
                    text: $form.pressing.country,
                    width: .short,
                ),
                FieldRow(
                    label: "Barcode",
                    placeholder: "Barcode",
                    text: $form.pressing.barcode,
                    width: .medium,
                    monospaced: true,
                ),
            ],
        )
    }

    // MARK: - Tracks

    private var tracksGroup: some View {
        VStack(alignment: .leading, spacing: 8) {
            sectionHeader(title: "Tracks", trailing: trackCountLabel)
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
        form.tracks.count == 1 ? "1 track" : "\(form.tracks.count) tracks"
    }

    private var trackHeaderRow: some View {
        HStack(spacing: 10) {
            eyebrow("#")
                .frame(width: trackOrdinalWidth)
            eyebrow("Title")
                .frame(maxWidth: .infinity, alignment: .leading)
            HStack(spacing: 4) {
                eyebrow("Artist")
                Text("· blank = album artist")
                    .font(.system(size: 10))
                    .foregroundStyle(.quaternary)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            eyebrow("Disc")
                .frame(width: trackNumberWidth)
            eyebrow("Track")
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
            Text("\(index + 1)")
                .font(.system(size: 12))
                .monospacedDigit()
                .foregroundStyle(.tertiary)
                .frame(width: trackOrdinalWidth)
            MetadataField(
                placeholder: "Title",
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
        form.albumArtistText.isEmpty ? "Artist" : form.albumArtistText
    }

    private let trackOrdinalWidth: CGFloat = 34
    private let trackNumberWidth: CGFloat = 58

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
            eyebrow(title, size: 11)
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
    private func eyebrow(_ text: String, size: CGFloat = 10) -> some View {
        Text(text)
            .font(.system(size: size, weight: .bold))
            .textCase(.uppercase)
            .tracking(1)
            .foregroundStyle(.tertiary)
    }
}

// MARK: - FieldRow

/// A label-left / value-right row spec for a grouped card. All album and
/// pressing fields are raw `String` text, so one spec drives every row.
private struct FieldRow {
    let label: String
    var hint: String?
    let placeholder: String
    let text: Binding<String>
    let width: FieldWidth
    var monospaced: Bool = false
}

private enum FieldWidth {
    case short
    case medium
    case long

    var maxWidth: CGFloat {
        switch self {
        case .short:
            160
        case .medium:
            300
        case .long:
            520
        }
    }
}

// MARK: - Editable fields

/// A `String` text field styled to the confirm-pane vocabulary: a recessed
/// well that lifts and gains an accent border on focus. `boxed` is the
/// grouped-card look (always-visible well); `boxed: false` is the track
/// table's borderless cell, transparent until focused.
private struct MetadataField: View {
    let placeholder: String
    @Binding
    var text: String
    var monospaced: Bool = false
    var boxed: Bool = true

    @FocusState
    private var focused: Bool

    var body: some View {
        field
            .modifier(FieldChrome(focused: focused, boxed: boxed))
    }

    @ViewBuilder
    private var field: some View {
        let base = TextField(placeholder, text: $text)
            .textFieldStyle(.plain)
            .font(.system(size: 13))
            .focused($focused)
        if monospaced {
            base.monospacedDigit()
        }
        else {
            base
        }
    }
}

/// The disc/side cell — a required `Int32`, centered tabular digits.
private struct TrackSideCell: View {
    @Binding
    var value: Int32

    @FocusState
    private var focused: Bool

    var body: some View {
        TextField("", value: $value, format: .number)
            .focused($focused)
            .modifier(NumericCellStyle(focused: focused))
    }
}

/// The track-number cell — an optional `Int32?` (blank when unset),
/// centered tabular digits.
private struct TrackNumberCell: View {
    @Binding
    var value: Int32?

    @FocusState
    private var focused: Bool

    var body: some View {
        TextField("", value: $value, format: .number)
            .focused($focused)
            .modifier(NumericCellStyle(focused: focused))
    }
}

/// Shared styling for the centered, tabular borderless numeric track cells —
/// the disc and track-number columns differ only in required vs optional
/// `Int32`, so the look lives here.
private struct NumericCellStyle: ViewModifier {
    let focused: Bool

    func body(content: Content) -> some View {
        content
            .textFieldStyle(.plain)
            .font(.system(size: 13))
            .monospacedDigit()
            .multilineTextAlignment(.center)
            .modifier(FieldChrome(focused: focused, boxed: false))
    }
}

/// The shared field chrome: recessed fill + hairline border that becomes a
/// lifted fill + accent border on focus. `boxed` controls the resting look;
/// the focused look is identical for boxed and borderless cells.
private struct FieldChrome: ViewModifier {
    let focused: Bool
    let boxed: Bool

    func body(content: Content) -> some View {
        content
            .padding(.horizontal, 10)
            .padding(.vertical, 6)
            .background(restingFill)
            .clipShape(RoundedRectangle(cornerRadius: 6))
            .overlay {
                RoundedRectangle(cornerRadius: 6)
                    .strokeBorder(borderColor, lineWidth: focused ? 1.5 : 1)
            }
    }

    private var restingFill: Color {
        if focused {
            return Theme.fieldHover
        }
        return boxed ? Theme.field : .clear
    }

    private var borderColor: Color {
        if focused {
            return Theme.accent
        }
        return boxed ? .white.opacity(0.07) : .clear
    }
}

// MARK: - Previews

#Preview("Edit Metadata Form") {
    @Previewable
    @State
    var form = BridgeRawReleaseEdit(
        albumTitle: "Album Title",
        albumArtistText: "Artist Name",
        pressing: BridgeRawPressingEdit(
            year: "1997",
            format: "CD",
            label: "Some Label",
            catalogNumber: "CAT-11601",
            country: "US",
            barcode: "008811160128"
        ),
        tracks: (1...13)
            .map { n in
                BridgeRawTrackEdit(
                    id: "t-\(n)",
                    title: "Track Title \(n)",
                    artistText: "",
                    side: 1,
                    trackNumber: Int32(n)
                )
            }
    )
    ScrollView {
        EditMetadataForm(form: $form)
            .padding(20)
    }
    .frame(width: 640, height: 720)
    .background(Theme.background)
    .preferredColorScheme(.dark)
}
