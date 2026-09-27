import BaeKit
import SwiftUI

/// The left half of a mapping row: what the folder offers for it — a file
/// whole, one entry of a track sheet, or nothing at all where the release names
/// a track this folder has no audio for.
struct ImportMappingSourceCell: View {
    static let auditionTargetSize: CGFloat = 24

    let source: BridgeMappingSource
    let previewingTarget: BridgePreviewTarget?
    /// Identifying signals extracted from this row's file.
    var evidence: [BridgeFileEvidence]
    /// Files rows show the file size; track rows don't.
    let showsFileSize: Bool
    let actions: ImportMappingActions

    private var isPreviewing: Bool {
        guard let previewTarget = source.previewTarget else { return false }
        return previewTarget == previewingTarget
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
                        RoundedRectangle(cornerRadius: ThemeRadius.chip)
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
            if let previewTarget = source.previewTarget {
                auditionButton(target: previewTarget)
            }
            nameCell(file)
            if showsFileSize {
                Text(file.sizeText)
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
                    // A squeezed column must truncate the name, never wrap the
                    // size mid-digit.
                    .fixedSize()
            }
            ForEach(ImportEvidence.badges(evidence)) { badge in
                ImportEvidenceChip(signal: badge.signal)
                    .fixedSize()
                    .help(ImportEvidence.hoverText(badge.evidence))
            }
            Spacer(minLength: 0)
        }
    }

    /// The file name in mono; a document's name opens it in the viewer.
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

    /// One track sheet entry's title and play button; its number is in the
    /// `#` column.
    private func entryCell(_ entry: BridgeMappingEntry) -> some View {
        HStack(spacing: 6) {
            if let previewTarget = source.previewTarget {
                auditionButton(target: previewTarget)
            }
            Text(entry.title ?? "")
                .font(.system(size: 12))
                .lineLimit(1)
                .truncationMode(.tail)
            Spacer(minLength: 0)
        }
    }

    private func auditionButton(target: BridgePreviewTarget) -> some View {
        Button {
            isPreviewing ? actions.stopPreview() : actions.preview(target)
        } label: {
            Image(systemName: isPreviewing ? "stop.fill" : "play.fill")
                .font(.system(size: 11, weight: .semibold))
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
