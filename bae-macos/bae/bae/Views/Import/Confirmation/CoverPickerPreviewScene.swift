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
                    remoteItems: Self.remoteItems,
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
            CoverLightboxPreviewScene(items: remoteItems + releaseItems)
                .environment(PreviewData.artImageStore())
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

    private struct CoverLightboxPreviewScene: View {
        @State
        private var cursor: Cursor<CoverItem>

        init(items: [CoverItem]) {
            guard let cursor = Cursor(items: items, preferring: items[1].id)
            else {
                preconditionFailure("The artwork preview contains a booklet")
            }
            _cursor = State(initialValue: cursor)
        }

        var body: some View {
            LightboxView(
                cursor: cursor,
                onUpdate: { cursor = $0 },
                onDismiss: {}
            )
            .background(Theme.background)
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
#endif
