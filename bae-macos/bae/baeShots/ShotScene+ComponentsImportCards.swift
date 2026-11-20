import BaeKit
import SwiftUI

@testable import bae

// The card a skipped folder shows in place of an import's draft card.

extension ShotScene {
    static let importCardScenes: [ShotScene] = invalidCandidateCardScenes

    /// The preview's 720pt box.
    private static let cardWidth: CGFloat = 720

    // MARK: Skipped folder

    /// A folder in `2024-rips` whose two audio files are damaged, called
    /// Low Tide Hours by Mara Quill when `tagged`, and nothing otherwise.
    private static func skippedFolder(tagged: Bool) -> BridgeInvalidCandidate {
        let path = Data("/Music/Downloads/2024-rips/untitled 07".utf8)
        return BridgeInvalidCandidate(
            candidateKey: path,
            folderPath: path,
            sourceFolderName: "untitled 07",
            watchedFolderPath: Data("/Music/Downloads".utf8),
            separable: false,
            reason: .damagedAudio(count: 2),
            tags: tagged
                ? BridgeSkippedTags(
                    albumTitle: "Low Tide Hours",
                    artist: .named("Mara Quill")
                ) : nil,
            cover: tagged
                ? .local(file: PreviewData.previewArtFile("Low Tide Hours"))
                : nil
        )
    }

    /// `MacInvalidCandidateCard`: a folder its tags name, with its own
    /// cover, and one they name nothing, by its folder's name.
    private static let invalidCandidateCardScenes: [ShotScene] = [
        component(
            "mac.component.invalid-candidate-card.tagged",
            width: cardWidth,
            height: 240,
            images: PreviewData.artImageStore()
        ) {
            MacInvalidCandidateCard(invalid: skippedFolder(tagged: true))
                .frame(width: 720, alignment: .leading)
        },
        component(
            "mac.component.invalid-candidate-card.no-tags",
            width: cardWidth,
            height: 240,
            images: PreviewData.artImageStore()
        ) {
            MacInvalidCandidateCard(invalid: skippedFolder(tagged: false))
                .frame(width: 720, alignment: .leading)
        },
    ]
}
