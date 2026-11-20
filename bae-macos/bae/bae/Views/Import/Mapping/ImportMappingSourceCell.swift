import BaeKit
import SwiftUI

/// The left half of an import's track row: what the folder offers for it, a
/// file whole or where one entry of a track sheet starts, and the button
/// that auditions it.
struct ImportMappingSourceCell: View {
    static let auditionTargetSize = ThemeSize.hitTarget

    let source: BridgeTrackSource
    /// The window of the folder's file the row auditions.
    let previewTarget: BridgePreviewTarget?
    let previewingTarget: BridgePreviewTarget?
    let actions: ImportMappingActions

    private var isPreviewing: Bool {
        guard let previewTarget else { return false }
        return previewTarget == previewingTarget
    }

    var body: some View {
        HStack(spacing: ThemeSpace.compact) {
            if let previewTarget {
                auditionButton(target: previewTarget)
            }
            switch source {
            case .file(let name):
                Text(name)
                    .themeText(.mono)
                    .cutLine(.fileName)
            case .sheetSlice(let start):
                // The entry's number is in the `#` column and its title in
                // the title column; the image it starts in is the caption's.
                TrackStartText(start: start)
            }
            Spacer(minLength: 0)
        }
        .frame(minHeight: Self.auditionTargetSize)
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
