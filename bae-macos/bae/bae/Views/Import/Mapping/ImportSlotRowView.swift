import BaeKit
import SwiftUI

/// One track slot: the source's position, the audio bound to it, the link glyph
/// carrying the pairing state, the editable title and artist, both lengths, and
/// the actions that change the pairing.
///
/// The two lengths are the point of the row. Counting cannot see a pairing that
/// is complete but wrong — thirteen files against thirteen tracks in the wrong
/// order counts perfectly — and reading the file's own length against the
/// source's is what catches it.
struct ImportSlotRowView: View {
    let row: ImportSlotRow
    let audioChoices: [BridgeSlotFile]
    let previewingPath: String?
    @Binding
    var track: BridgeRawTrackEdit
    let actions: ImportSlotActions

    private var isPreviewing: Bool {
        row.file.map { $0.localPath == previewingPath } ?? false
    }

    var body: some View {
        HStack(spacing: 10) {
            Text(row.position ?? "")
                .font(.system(size: 12))
                .monospacedDigit()
                .foregroundStyle(.tertiary)
                .frame(width: ImportSlotColumns.position, alignment: .leading)
            fileCell
                .frame(width: ImportSlotColumns.file, alignment: .leading)
            ImportSlotLinkGlyph(
                file: row.file,
                hasPosition: row.position != nil
            )
            .frame(width: ImportSlotColumns.link)
            MetadataField(
                placeholder: coreString("ui.import.slots.untitled"),
                text: $track.title,
                boxed: false,
            )
            .frame(maxWidth: .infinity)
            MetadataField(
                placeholder: String(localized: "Artist"),
                text: $track.artistText,
                boxed: false,
            )
            .frame(maxWidth: .infinity)
            ImportSlotLengths(
                probedMs: row.file?.probedDurationMs,
                sourceMs: row.sourceDurationMs
            )
            .frame(width: ImportSlotColumns.length, alignment: .trailing)
            ImportSlotRowActions(
                row: row,
                audioChoices: audioChoices,
                actions: actions,
            )
            .frame(width: ImportSlotColumns.actions, alignment: .trailing)
        }
    }

    /// The audition control, then the file's own name and the container's size.
    /// A slot with nothing behind it says so instead.
    @ViewBuilder
    private var fileCell: some View {
        if let file = row.file {
            HStack(spacing: 6) {
                Button {
                    isPreviewing
                        ? actions.stopPreview()
                        : actions.preview(file.localPath)
                } label: {
                    Image(
                        systemName: isPreviewing ? "stop.fill" : "play.fill"
                    )
                    .font(.system(size: 9))
                    .foregroundStyle(
                        isPreviewing
                            ? AnyShapeStyle(Theme.accent)
                            : AnyShapeStyle(.secondary)
                    )
                    .frame(width: 14, height: 14)
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .help(
                    isPreviewing
                        ? coreString("ui.import.slots.stop")
                        : coreString("ui.import.slots.play")
                )
                Text(file.name)
                    .font(.system(size: 12, design: .monospaced))
                    .lineLimit(1)
                    .truncationMode(.middle)
                Spacer(minLength: 4)
                Text(file.sizeText)
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
            }
        }
        else {
            Text(coreString("ui.import.slots.no_file"))
                .font(.system(size: 12))
                .foregroundStyle(.quaternary)
                .padding(.horizontal, 8)
                .padding(.vertical, 3)
                .overlay {
                    RoundedRectangle(cornerRadius: 5)
                        .strokeBorder(
                            style: StrokeStyle(lineWidth: 1, dash: [3, 3])
                        )
                        .foregroundStyle(.quaternary)
                }
        }
    }
}
