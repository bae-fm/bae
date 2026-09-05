import AppKit
import BaeKit
import SwiftUI
import Testing
import Vision

@testable import bae

@Suite("Cover picker")
struct CoverPickerTests {
    private actor CoverRecorder {
        var reads: [String] = []
        var selected: BridgeCoverSelection?

        func read(_ releaseId: String, _ source: BridgeGallerySource) {
            if case .releaseFile(let fileId) = source {
                reads.append("\(releaseId)/\(fileId)")
            }
        }

        func select(_ selection: BridgeCoverSelection) { selected = selection }
    }

    @MainActor
    @Test("Persisted artwork is previewed and saved by library file ID")
    func persistedArtworkUsesLibraryIdentity() async throws {
        let release = persistedRelease()
        let recorder = CoverRecorder()
        let library = Library(subscribeReleaseDetail: { _, callback in
            callback.onValue(value: release)
            return TestLiveSubscription(Task {})
        })
        let png = try imageBytes()
        let images = ImageStore(fetchReleaseImageBytes: { releaseId, source in
            await recorder.read(releaseId, source)
            return png
        })
        let size = NSSize(width: 960, height: 700)
        let (window, host) = SnapshotTestSupport.hostInWindow(
            CoverSheetView(
                releaseId: release.id,
                fetchRemoteCovers: { [] },
                onSelect: { await recorder.select($0) },
                onDone: {}
            )
            .environment(library)
            .environment(images)
            .frame(width: size.width, height: size.height),
            size: size
        )
        defer {
            window.contentView = nil
            window.orderOut(nil)
        }
        for _ in 0..<100 {
            await SnapshotTestSupport.settle(host)
            if !(await recorder.reads).isEmpty { break }
        }
        #expect(await recorder.reads.contains("release-test/library-image-id"))
        await SnapshotTestSupport.settle(host)
        let enter = try #require(
            NSEvent.keyEvent(
                with: .keyDown,
                location: .zero,
                modifierFlags: [],
                timestamp: 0,
                windowNumber: window.windowNumber,
                context: nil,
                characters: "\r",
                charactersIgnoringModifiers: "\r",
                isARepeat: false,
                keyCode: 36
            )
        )
        _ = host.performKeyEquivalent(with: enter)
        for _ in 0..<100 {
            await Task.yield()
            if await recorder.selected != nil { break }
        }
        #expect(
            await recorder.selected == .releaseImage(fileId: "library-image-id")
        )
    }

    private func persistedRelease() -> BridgeRelease {
        let file = BridgeFile(
            id: "library-image-id",
            originalFilename: "scans/front.png",
            fileSize: 100,
            contentType: "image/png",
            isImage: true,
            audioFormat: nil
        )
        return BridgeRelease(
            id: "release-test",
            albumId: "album-test",
            displayName: "Release",
            year: nil,
            format: nil,
            label: nil,
            catalogNumber: nil,
            country: nil,
            storageState: .remote,
            pinned: false,
            storageActions: [],
            transferAction: nil,
            tracks: [],
            trackGroups: [],
            files: [file],
            sourceAudio: nil,
            imageFiles: [file],
            galleryItems: [],
            totalDuration: nil,
            fileCount: 1,
            totalSize: 100,
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

    @MainActor
    @Test("Import cover picker labels external sources and release files")
    func sourceSections() async throws {
        let size = NSSize(width: 960, height: 700)
        let (window, host) = SnapshotTestSupport.hostInWindow(
            CoverPickerView(
                remoteCoverArts: PreviewData.remoteCovers,
                localArtwork: PreviewData.bridgeCandidateFiles.images,
                selectedCover: nil,
                onSelect: { _ in },
                onDone: {}
            )
            .environment(ImageStore.stub())
            .frame(width: size.width, height: size.height),
            size: size
        )
        defer {
            window.contentView = nil
            window.orderOut(nil)
        }
        await SnapshotTestSupport.settle(host)
        let png = try await SnapshotTestSupport.capturePNG(host, size: size)
        let request = VNRecognizeTextRequest()
        request.recognitionLevel = .accurate
        try VNImageRequestHandler(data: png, options: [:]).perform([request])
        let results = try #require(request.results)
        let labels = results.compactMap { $0.topCandidates(1).first?.string }
        #expect(labels.contains(String(localized: "Remote Sources")))
        #expect(labels.contains(String(localized: "Release Files")))
        #expect(labels.contains(bridgeMetadataSourceName(source: .musicBrainz)))
    }
}
