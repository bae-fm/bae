import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacFileNameField`: the file name pattern with its preview, the folder
    /// pattern, and a pattern the name can't be made from.
    static let macFileNameFieldScenes: [ShotScene] = [
        component("mac.component.file-name-field.file-name", height: 200) {
            ShotBox {
                MacFileNameField(
                    title: "File name",
                    pieces: PreviewData.exportFilenamePieces,
                    error: nil,
                    previewSuffix: ".flac",
                    setPieces: { _ in },
                    fail: { _ in }
                )
            }
        },
        component("mac.component.file-name-field.typed-text", height: 200) {
            ShotBox {
                MacFileNameField(
                    title: "Folder name",
                    pieces: PreviewData.exportFolderPieces,
                    error: nil,
                    setPieces: { _ in },
                    fail: { _ in }
                )
            }
        },
        component("mac.component.file-name-field.error", height: 200) {
            ShotBox {
                MacFileNameField<BridgeSaveFolderPiece>(
                    title: "Folder name",
                    pieces: [
                        .value(value: .artist), .text(text: " / "),
                        .value(value: .album),
                    ],
                    error: .forbiddenCharacter(character: "/"),
                    setPieces: { _ in },
                    fail: { _ in }
                )
            }
        },
    ]
}
