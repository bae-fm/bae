#if DEBUG
    import BaeKit
    import SwiftUI

    /// The production gallery over decoded fixture artwork, shared by Xcode
    /// previews and the screenshot gallery. Its images come from the
    /// `ImageStore` its environment holds.
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

        @State
        private var lightboxLoad = LightboxImageLoad()

        var body: some View {
            CoverPickerFrame {
                CoverGalleryView(
                    remoteItems: .linked(Self.remoteItems),
                    releaseItems: Self.releaseItems,
                    selectedCover: Self.releaseItems[0].selection,
                    lightboxLoad: lightboxLoad,
                    onRefresh: {},
                    onSelect: { _ in },
                    onDone: {}
                )
            }
            .background(Theme.background)
        }

        /// The gallery with its lookup in `remote`, saving the cover picked
        /// when `isSaving`, its images from `images`.
        static func lookup(
            _ remote: RemoteCoverItems,
            isSaving: Bool = false,
            images: ImageStore
        ) -> some View {
            CoverPickerFrame {
                CoverGalleryView(
                    remoteItems: remote,
                    releaseItems: releaseItems,
                    selectedCover: releaseItems[0].selection,
                    lightboxLoad: LightboxImageLoad(),
                    isSaving: isSaving,
                    onRefresh: {},
                    onSelect: { _ in },
                    onDone: {}
                )
            }
            .environment(images)
            .background(Theme.background)
        }

        /// The gallery open on its lightbox, its image loading in `load` and
        /// its strip's from `images`.
        static func lightbox(
            load: LightboxImageLoad,
            images: ImageStore
        ) -> some View {
            CoverGalleryView(
                remoteItems: .linked(remoteItems),
                releaseItems: releaseItems,
                selectedCover: remoteItems[1].selection,
                lightboxLoad: load,
                initialLayout: .lightbox,
                onRefresh: {},
                onSelect: { _ in },
                onDone: {}
            )
            .environment(images)
        }

        /// The gallery of a release linked to no catalog release, its images
        /// from `images`.
        static func unlinked(images: ImageStore) -> some View {
            CoverPickerFrame {
                CoverGalleryView(
                    remoteItems: .unlinked,
                    releaseItems: [releaseItems[0]],
                    selectedCover: releaseItems[0].selection,
                    lightboxLoad: LightboxImageLoad(),
                    onFindRelease: {},
                    onSelect: { _ in },
                    onDone: {}
                )
            }
            .environment(images)
            .background(Theme.background)
        }

        private static func remote(
            _ image: String,
            label: String,
            source: BridgeCatalog
        ) -> CoverItem {
            let image = BridgeRemoteImageSet(
                url: PreviewData.previewArtPath(image),
                downscaled: []
            )
            return CoverItem(
                coverChoice: BridgeCoverChoice(
                    selection: .remoteCover(
                        selection: BridgeRemoteCoverSelection(
                            image: image,
                            source: source
                        )
                    ),
                    image: .remote(image: image)
                ),
                label: label
            )
        }

        private static func local(_ image: String, filename: String)
            -> CoverItem
        {
            CoverItem(
                coverChoice: BridgeCoverChoice(
                    selection: .releaseImage(fileId: filename),
                    image: .local(file: PreviewData.previewArtFile(image))
                ),
                label: filename
            )
        }
    }

    #Preview("Cover gallery") {
        CoverPickerPreviewScene()
            .environment(PreviewData.artImageStore())
            .frame(width: 1_148, height: 868)
    }

    #Preview("Cover gallery — short window") {
        CoverPickerPreviewScene()
            .environment(PreviewData.artImageStore())
            .frame(width: 800, height: 520)
    }

    #Preview("Artwork lightbox") {
        CoverPickerPreviewScene.lightbox(
            load: LightboxImageLoad(),
            images: PreviewData.artImageStore()
        )
        .frame(width: 1_148, height: 868)
    }

    #Preview("Cover gallery — unlinked release") {
        CoverPickerPreviewScene.unlinked(images: PreviewData.artImageStore())
            .frame(width: 1_148, height: 868)
    }

    #Preview("Cover gallery — fetching covers") {
        CoverPickerPreviewScene.lookup(
            .loading([]),
            images: PreviewData.artImageStore()
        )
        .frame(width: 1_148, height: 868)
    }

    #Preview("Cover gallery — saving") {
        CoverPickerPreviewScene.lookup(
            .linked([]),
            isSaving: true,
            images: PreviewData.artImageStore()
        )
        .frame(width: 1_148, height: 868)
    }
#endif
