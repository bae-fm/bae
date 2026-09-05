#if DEBUG
    import BaeKit
    import SwiftUI

    /// The production gallery over decoded fixture artwork, shared by Xcode
    /// previews and the screenshot gallery.
    struct CoverPickerPreviewScene: View {
        private static let remoteItems = [
            remote(
                "Front",
                label: "Cover Art Archive · Front",
                source: .musicBrainz
            ),
            remote(
                "Booklet",
                label: "Cover Art Archive · Booklet",
                source: .musicBrainz
            ),
            remote("Discogs", label: "Discogs · [r123] · 1", source: .discogs),
        ]
        private static let releaseItems = [
            local("Front scan", filename: "Artwork/01 - Front cover.png"),
            local(
                "Back scan",
                filename:
                    "Artwork/02 - Back cover with liner notes and recording credits.png"
            ),
            local("Disc scan", filename: "Artwork/03 - Disc label.png"),
            local(
                "Booklet scan",
                filename: "Artwork/04 - Booklet pages 01–02.png"
            ),
        ]

        var body: some View {
            CoverPickerFrame {
                CoverGalleryView(
                    remoteItems: .linked(Self.remoteItems),
                    releaseItems: Self.releaseItems,
                    selectedCover: Self.releaseItems[0].selection,
                    onRefresh: {},
                    onSelect: { _ in },
                    onDone: {}
                )
            }
            .environment(PreviewData.artImageStore())
            .background(Theme.background)
        }

        static func lightbox() -> some View {
            CoverGalleryView(
                remoteItems: .linked(remoteItems),
                releaseItems: releaseItems,
                selectedCover: remoteItems[1].selection,
                initialLayout: .lightbox,
                onRefresh: {},
                onSelect: { _ in },
                onDone: {}
            )
            .environment(PreviewData.artImageStore())
        }

        static func unlinked() -> some View {
            CoverPickerFrame {
                CoverGalleryView(
                    remoteItems: .unlinked,
                    releaseItems: [releaseItems[0]],
                    selectedCover: releaseItems[0].selection,
                    onFindRelease: {},
                    onSelect: { _ in },
                    onDone: {}
                )
            }
            .environment(PreviewData.artImageStore())
            .background(Theme.background)
        }

        private static func remote(
            _ image: String,
            label: String,
            source: BridgeMetadataSource
        ) -> CoverItem {
            let path = PreviewData.previewArtPath(image)
            return CoverItem(
                coverChoice: BridgeCoverChoice(
                    selection: .remoteCover(
                        selection: BridgeRemoteCoverSelection(
                            url: path,
                            source: source
                        )
                    ),
                    previewSource: .remote(url: path),
                    thumbnailSource: .remote(url: path)
                ),
                label: label
            )
        }

        private static func local(_ image: String, filename: String)
            -> CoverItem
        {
            let path = PreviewData.previewArtPath(image)
            return CoverItem(
                coverChoice: BridgeCoverChoice(
                    selection: .releaseImage(fileId: filename),
                    previewSource: .local(path: path),
                    thumbnailSource: .local(path: path)
                ),
                label: filename
            )
        }
    }

    #Preview("Cover gallery") {
        CoverPickerPreviewScene().frame(width: 1_148, height: 868)
    }

    #Preview("Cover gallery — short window") {
        CoverPickerPreviewScene().frame(width: 800, height: 520)
    }

    #Preview("Artwork lightbox") {
        CoverPickerPreviewScene.lightbox().frame(width: 1_148, height: 868)
    }

    #Preview("Cover gallery — unlinked release") {
        CoverPickerPreviewScene.unlinked().frame(width: 1_148, height: 868)
    }
#endif
