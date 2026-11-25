import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The lightbox over an import folder's images (`mac.import.lightbox`),
    /// over the window `mac.import.folder.identified` draws, opened on
    /// Northbound's cover.jpg.
    static let importLightboxScenes: [ShotScene] = [
        ShotScene(
            id: "mac.import.lightbox",
            size: importLightboxSize,
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            try importLightboxStage(picture: .shown)
        },
        ShotScene(
            id: "mac.import.lightbox.loading",
            size: importLightboxSize,
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            try importLightboxStage(picture: .loading)
        },
        ShotScene(
            id: "mac.import.lightbox.failed",
            size: importLightboxSize,
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            try importLightboxStage(picture: .unreadable)
        },
    ]

    private static let importLightboxSize = CGSize(width: 1_400, height: 1_100)

    /// How core's read of the picture at the lightbox's size stands.
    private enum FolderPicture {
        /// Core reads every picture.
        case shown
        /// Core has not answered yet.
        case loading
        /// Core's read of the folder's file fails, with its error behind
        /// Details.
        case unreadable
    }

    /// The widest the import pane asks for a picture, in pixels; the lightbox
    /// asks for more.
    nonisolated private static let panePixels: UInt32 = 1_600

    @MainActor
    private static func importLightboxStage(
        picture: FolderPicture
    ) throws -> ShotStage {
        let size = importLightboxSize
        let uiStore = UiStore()
        let images = folderImages(picture)
        let load = LightboxImageLoad()
        uiStore.presentLightbox(try northboundLightbox(uiStore: uiStore))
        return ShotStage(
            try importWindow(
                .found(northbound: .identified, opening: .northbound),
                uiStore: uiStore,
                images: images,
                size: size,
                window: AnyView(MacMainWindowContainer(lightboxLoad: load))
            ),
            isReady: {
                switch picture {
                case .shown:
                    load.phase.image != nil && images.unansweredImages == 0
                case .loading:
                    // Every image but the one that never answers.
                    load.content != nil && images.unansweredImages == 1
                case .unreadable:
                    if case .failed = load.phase {
                        images.unansweredImages == 0
                    }
                    else {
                        false
                    }
                }
            }
        )
    }

    /// The lightbox on the first of Northbound's two images.
    @MainActor
    private static func northboundLightbox(uiStore: UiStore) throws
        -> FolderImagesLightbox
    {
        let images = try PreviewData.northboundImages(
            cover: PreviewData.northboundCover()
        )
        let key = PreviewData.BoardFolder.northbound.key
        let request = BridgeFolderImagesRequest(
            candidateKey: key,
            focus: images.cover.fileId
        )
        return FolderImagesLightbox(
            candidateKey: key,
            focusedImage: images.cover.fileId,
            query: FolderImagesQuery(
                setRequest: { _ in },
                values: .fixed(
                    BridgeFolderImagesSnapshot(
                        request: request,
                        images: BridgeFolderImages(
                            images: images.gallery,
                            focus: BridgeFolderImageFocus(
                                image: images.cover,
                                neighbors: BridgeFolderImageNeighbors(
                                    previous: images.back.fileId,
                                    next: images.back.fileId
                                )
                            )
                        )
                    )
                )
            ),
            onError: { uiStore.showError($0) }
        )
    }

    /// The import window's art, whose copies at the lightbox's size read as
    /// `picture` says.
    private static func folderImages(_ picture: FolderPicture) -> ImageStore {
        PreviewData.artImageStore { pixels in
            guard pixels > panePixels else { return }
            switch picture {
            case .shown: break
            case .loading: try await Task.neverAnswers()
            case .unreadable:
                throw BridgeError.Diagnostic(
                    category: .import,
                    detail: "opening cover.jpg failed: permission denied"
                )
            }
        }
    }
}
