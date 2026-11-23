import BaeKit
import SwiftUI

/// The left cell of a track row: the button that plays the track, then
/// what the track plays from, a file whole or where its CUE sheet's slice
/// starts in the file the sheet's caption names.
struct TrackSourceCell: View {
    let source: BridgeTrackSource
    let audition: BridgeTrackAudition
    /// The track the row edits, which a library release's row plays from.
    let trackId: String
    let previewingTarget: BridgePreviewTarget?
    let actions: TrackAuditionActions
    private static let auditionLineHeight: CGFloat = 16

    private var isPreviewing: Bool {
        guard let target = audition.previewTarget else { return false }
        return target == previewingTarget
    }

    var body: some View {
        HStack(spacing: ThemeSpace.related) {
            playButton
            switch source {
            case .file(let name):
                Text(verbatim: name)
                    .themeText(.mono)
                    .cutLine(.fileName)
                    .help(name)
            case .sheetSlice(let start):
                // The entry's number is in the `#` column and its title in
                // the title column; the image it starts in is the caption's.
                TrackStartText(start: start)
            }
            Spacer(minLength: 0)
        }
    }

    @ViewBuilder
    private var playButton: some View {
        switch audition {
        case .preview(let target):
            MacAuditionButton(
                isPlaying: isPreviewing,
                height: Self.auditionLineHeight
            ) {
                isPreviewing ? actions.stopPreview() : actions.preview(target)
            }
        case .libraryTrack:
            if let playTrack = actions.playTrack {
                MacAuditionButton(
                    isPlaying: false,
                    height: Self.auditionLineHeight
                ) {
                    playTrack(trackId)
                }
            }
        case .unsaved:
            EmptyView()
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
