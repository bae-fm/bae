import BaeKit
import SwiftUI

/// The left half of a mapping row: what the folder offers for it — a file
/// whole, or one entry of a track sheet.
struct ImportMappingSourceCell: View {
    static let auditionTargetSize = ThemeSize.hitTarget

    let source: BridgeMappingSource
    let previewingTarget: BridgePreviewTarget?
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
            if let damage = file.damageText {
                Text(damage)
                    .themeText(.fine)
                    .foregroundStyle(Theme.warning)
                    .fixedSize()
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
            .buttonStyle(.accentPlain)
        }
        else {
            nameLine(file)
        }
    }

    private func nameLine(_ file: BridgeMappingFile) -> some View {
        Text(file.name)
            .themeText(.mono)
            .cutLine(.fileName)
    }

    private func openAction(_ file: BridgeMappingFile) -> (() -> Void)? {
        guard file.role == .document else { return nil }
        return { actions.openDocument(file.name, file.fileId) }
    }

    /// One track sheet entry's play button and where it starts in the image
    /// its sheet's caption names; its number is in the `#` column and its
    /// title in the title column.
    private func entryCell(_ entry: BridgeMappingEntry) -> some View {
        HStack(spacing: ThemeSpace.compact) {
            if let previewTarget = source.previewTarget {
                auditionButton(target: previewTarget)
            }
            TrackStartText(start: entry.start)
            Spacer(minLength: 0)
        }
    }

    private func auditionButton(target: BridgePreviewTarget) -> some View {
        MacAuditionButton(
            isPlaying: isPreviewing,
            hitSize: Self.auditionTargetSize
        ) {
            isPreviewing ? actions.stopPreview() : actions.preview(target)
        }
    }
}

extension BridgeMappingFile {
    /// What is wrong with a damaged file, in the user's language; nil for
    /// one that is whole.
    var damageText: String? {
        switch role {
        case .damagedImage:
            localizedCoreString("ui.import.mapping.damaged_image")
        case .damagedAudio(let damage):
            damage.localizedText
        case .audio, .document, .image, .other:
            nil
        }
    }
}
