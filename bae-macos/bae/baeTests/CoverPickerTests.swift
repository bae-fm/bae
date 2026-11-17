import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

@Suite("Cover picker")
struct CoverPickerTests {
    private actor CoverRecorder {
        var reads: [String] = []
        var selected: BridgeCoverSelection?

        func read(_ releaseId: String, _ source: BridgeGallerySource) {
            if case .releaseFile(let fileId) = source {
                reads.append("\(releaseId)/\(fileId)")
                Wait.happened()
            }
        }

        func select(_ selection: BridgeCoverSelection) {
            selected = selection
            Wait.happened()
        }
    }

    @MainActor
    @Test("Persisted artwork is previewed and saved by library file ID")
    func persistedArtworkUsesLibraryIdentity() async throws {
        let release = persistedRelease()
        let recorder = CoverRecorder()
        let png = try imageBytes()
        let images = ImageStore(fetchReleaseImageBytes: { releaseId, source in
            await recorder.read(releaseId, source)
            return png
        })
        let size = NSSize(width: 960, height: 700)
        try await SnapshotTestSupport.withHostedWindow(
            CoverSheetView(
                owner: .release(releaseId: release.id),
                lightboxLoad: LightboxImageLoad(),
                refreshCovers: {},
                onSelect: { await recorder.select($0) },
                onDone: {}
            )
            .environment(images)
            .environment(
                Library(coverGallery: { .fixed(gallery(of: release)) })
            )
            .frame(width: size.width, height: size.height),
            size: size
        ) { _, host in
            try await Wait.until { !(await recorder.reads).isEmpty }
            try await SnapshotTestSupport.settle(host)
            #expect(
                await recorder.reads.contains("release-test/library-image-id")
            )
            try await SnapshotTestSupport.settle(host)
            #expect(
                !(await recorder.reads).contains("release-test/bitmap-image-id")
            )
            try HostedInput.keyEquivalent(.return, in: host)
            try await Wait.until { await recorder.selected != nil }
            #expect(
                await recorder.selected
                    == .releaseImage(fileId: "library-image-id")
            )
        }
    }

    /// The covers core reads for `release`: its image files that can be a
    /// cover. Saving one selects it by its library file id.
    private func gallery(of release: BridgeRelease) -> BridgeCoverGallery {
        let files = release.coverFiles.map {
            BridgeGalleryCover(
                key: .file(fileId: $0.id),
                selection: .releaseImage(fileId: $0.id),
                image: .releaseImage(
                    releaseId: release.id,
                    source: .releaseFile(fileId: $0.id)
                ),
                label: .file(name: $0.originalFilename)
            )
        }
        return BridgeCoverGallery(
            current: nil,
            fetch: .unlinked,
            sections: [
                BridgeCoverSection(source: .remote, items: [], count: 0),
                BridgeCoverSection(
                    source: .releaseFiles,
                    items: files,
                    count: UInt32(files.count)
                ),
            ],
            total: UInt32(files.count),
            focus: nil
        )
    }

    private func persistedRelease() -> BridgeRelease {
        let file = BridgeFile(
            id: "library-image-id",
            originalFilename: "scans/front.png",
            name: "front.png",
            folder: "scans",
            kind: .image(format: "PNG"),
            fileSize: 100,
            contentType: "image/png",
            isImage: true,
            audioFormat: nil
        )
        let bitmap = BridgeFile(
            id: "bitmap-image-id",
            originalFilename: "scans/booklet.bmp",
            name: "booklet.bmp",
            folder: "scans",
            kind: .image(format: "BMP"),
            fileSize: 120,
            contentType: "image/bmp",
            isImage: true,
            audioFormat: nil
        )
        let facts = PreviewData.pressingFacts()
        return BridgeRelease(
            id: "release-test",
            albumId: "album-test",
            name: .named(name: "Release"),
            distinctYear: nil,
            labels: [],
            facts: facts,
            pressingSummary: bridgePressingSummary(facts: facts),
            mediaTerms: bridgeMediaTerms(media: facts.media),
            pressingDetails: bridgePressingDetails(facts: facts),
            storageState: .inStorage(pinned: false),
            storageActions: [],
            upload: nil,
            transitioning: false,
            tracks: [],
            trackGroups: [],
            credits: [],
            performsWorks: false,
            sourceAudio: nil,
            otherFiles: [],
            hasOtherFiles: false,
            imageFiles: [file, bitmap],
            coverFiles: [file],
            galleryItems: [],
            playability: .playable,
            moveToThisDevice: nil,
            downloadStatus: nil,
            records: [],
            hasRecords: false,
            totalDuration: nil,
            fileCount: 2,
            totalSize: 220,
            cover: nil
        )
    }

    @MainActor
    private func imageBytes() throws -> Data {
        let bitmap = try #require(
            NSBitmapImageRep(
                bitmapDataPlanes: nil,
                pixelsWide: 1,
                pixelsHigh: 1,
                bitsPerSample: 8,
                samplesPerPixel: 4,
                hasAlpha: true,
                isPlanar: false,
                colorSpaceName: .deviceRGB,
                bytesPerRow: 4,
                bitsPerPixel: 32
            )
        )
        bitmap.setColor(.blue, atX: 0, y: 0)
        return try #require(
            bitmap.representation(using: .png, properties: [:])
        )
    }

    /// Each tile names its source. The `cover-picker-wide` shot checks how
    /// the sections look.
    @MainActor
    @Test("Import cover picker labels external sources and release files")
    func sourceSections() throws {
        let remote = try #require(PreviewData.remoteCovers.first)
        let remoteItem = CoverItem(
            gallery: BridgeGalleryCover(
                key: .remote(url: "front"),
                selection: remote.coverChoice.selection,
                image: remote.coverChoice.image,
                label: .remote(
                    sourceName: remote.sourceName,
                    label: remote.label
                )
            )
        )
        #expect(
            remoteItem.sourceLabel == bridgeCatalogName(catalog: .musicBrainz)
        )
        let fileItem = CoverItem(
            gallery: BridgeGalleryCover(
                key: .file(fileId: "front.png"),
                selection: .releaseImage(fileId: "front.png"),
                image: .local(file: PreviewData.previewArtFile("front.png")),
                label: .file(name: "front.png")
            )
        )
        #expect(fileItem.sourceLabel == String(localized: "Release files"))
    }
}
