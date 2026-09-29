import BaeKit
import SwiftUI

/// The left half of a mapping row: what the folder offers for it — a file
/// whole, or one entry of a track sheet.
struct ImportMappingSourceCell: View {
    static let auditionTargetSize = ThemeSize.hitTarget

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
            }
        }
        .frame(minHeight: Self.auditionTargetSize)
    }

    private func fileCell(_ file: BridgeMappingFile) -> some View {
        HStack(spacing: ThemeSpace.compact) {
            if let previewTarget = source.previewTarget {
                auditionButton(target: previewTarget)
            }
            nameCell(file)
            if showsFileSize {
                Text(file.sizeText)
                    .themeText(.fine)
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
            .themeText(.mono)
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
        HStack(spacing: ThemeSpace.compact) {
            if let previewTarget = source.previewTarget {
                auditionButton(target: previewTarget)
            }
            Text(entry.title ?? "")
                .themeText(.body)
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
                .themeIcon(.small)
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
