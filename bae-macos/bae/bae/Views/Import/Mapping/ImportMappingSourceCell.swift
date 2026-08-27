import BaeKit
import SwiftUI

/// The left half of a mapping row: what the folder offers for it — a file
/// whole, one entry of a track sheet, or nothing at all where the release names
/// a track this folder has no audio for.
struct ImportMappingSourceCell: View {
    static let auditionTargetSize: CGFloat = 24

    let source: BridgeMappingSource
    let previewingPath: String?
    /// Whether the folder and the release disagree about how long this row
    /// runs, which is what marks a sheet entry's own length.
    let lengthsDiverge: Bool
    /// Whether this row's audio has not been read yet. Its length is the one
    /// thing on the pane that is still being fetched, so it is the one place a
    /// spinner belongs.
    var isMeasuring: Bool = false
    /// Identifying signals extracted from this row's file. Empty for every
    /// other row.
    var evidence: [BridgeFileEvidence]
    let actions: ImportMappingActions

    private var isPreviewing: Bool {
        source.audioPath.map { $0 == previewingPath } ?? false
    }

    var body: some View {
        Group {
            switch source {
            case .file(let file):
                fileCell(file)
            case .sheetEntry(let entry):
                entryCell(entry)
            case .missing:
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
        .frame(minHeight: Self.auditionTargetSize)
    }

    private func fileCell(_ file: BridgeMappingFile) -> some View {
        HStack(spacing: 6) {
            if file.role.fileRole.isAudio {
                auditionButton(path: file.localPath)
            }
            nameCell(file)
            Text(file.sizeText)
                .font(.caption2)
                .foregroundStyle(.tertiary)
                // A squeezed column must truncate the name, never wrap the
                // size mid-digit.
                .fixedSize()
            ForEach(ImportEvidence.badges(evidence)) { badge in
                ImportEvidenceChip(signal: badge.signal)
                    .fixedSize()
                    .help(ImportEvidence.hoverText(badge.evidence))
            }
            if isMeasuring {
                ProgressView()
                    .controlSize(.mini)
                    .help(String(localized: "Reading how long this runs"))
            }
            Spacer(minLength: 0)
        }
    }

    /// The file's own name in mono. Opening it is the affordance where there is
    /// something to open, which is a document in the viewer.
    @ViewBuilder
    private func nameCell(_ file: BridgeMappingFile) -> some View {
        if let open = openAction(file) {
            Button(action: open) {
                nameLine(file).contentShape(Rectangle())
            }
            .buttonStyle(.plain)
        }
        else {
            nameLine(file)
        }
    }

    private func nameLine(_ file: BridgeMappingFile) -> some View {
        Text(file.name)
            .font(.system(size: 12, design: .monospaced))
            .lineLimit(1)
            .truncationMode(.middle)
    }

    private func openAction(_ file: BridgeMappingFile) -> (() -> Void)? {
        guard file.role.fileRole.isDocument else { return nil }
        return { actions.openDocument(file.name, file.localPath) }
    }

    /// One entry of a track sheet: the number it prints, the title it gives,
    /// and how long it says the entry runs. The audio is the container's, which
    /// is the only file on disk there is to audition.
    private func entryCell(_ entry: BridgeMappingEntry) -> some View {
        HStack(spacing: 6) {
            auditionButton(path: entry.containerLocalPath)
            Text(verbatim: "\(entry.number).")
                .font(.system(size: 12, design: .monospaced))
                .monospacedDigit()
                .foregroundStyle(.tertiary)
            Text(entry.title ?? "")
                .font(.system(size: 12))
                .lineLimit(1)
                .truncationMode(.tail)
            if isMeasuring {
                ProgressView()
                    .controlSize(.mini)
                    .help(String(localized: "Reading how long this runs"))
            }
            else {
                Text(importDurationText(entry.durationMs))
                    .font(.caption2)
                    .monospacedDigit()
                    .fixedSize()
                    .foregroundStyle(
                        lengthsDiverge
                            ? AnyShapeStyle(.orange) : AnyShapeStyle(.tertiary)
                    )
            }
            Spacer(minLength: 0)
        }
    }

    private func auditionButton(path: String) -> some View {
        Button {
            isPreviewing ? actions.stopPreview() : actions.preview(path)
        } label: {
            Image(systemName: isPreviewing ? "stop.fill" : "play.fill")
                .font(.system(size: 12, weight: .semibold))
                .foregroundStyle(
                    isPreviewing
                        ? AnyShapeStyle(Theme.accent)
                        : AnyShapeStyle(.secondary)
                )
                .frame(
                    width: Self.auditionTargetSize,
                    height: Self.auditionTargetSize
                )
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .help(
            isPreviewing
                ? coreString("ui.import.slots.stop")
                : coreString("ui.import.slots.play")
        )
        .accessibilityLabel(
            isPreviewing
                ? coreString("ui.import.slots.stop")
                : coreString("ui.import.slots.play")
        )
    }
}
