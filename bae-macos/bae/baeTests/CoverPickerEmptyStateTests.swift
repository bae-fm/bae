import AppKit
import BaeKit
import SwiftUI
import Testing
import Vision

@testable import bae

@MainActor
struct CoverPickerEmptyStateTests {
    @Test(
        "Only a completed linked lookup says no remote covers were found",
        arguments: [
            RemoteCoverItems.unlinked, .linked([]), .loading([]),
            .failed([], message: "Artwork lookup failed"),
        ]
    )
    func lookupMessages(_ remoteItems: RemoteCoverItems) async throws {
        let size = NSSize(width: 960, height: 700)
        let (window, host) = SnapshotTestSupport.hostInWindow(
            CoverGalleryView(
                remoteItems: remoteItems,
                releaseItems: [],
                selectedCover: nil,
                onRefresh: {},
                onFindRelease: {},
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
        let observations = try await text(in: host, size: size)
        let labels = observations.compactMap {
            $0.topCandidates(1).first?.string
        }
        #expect(
            labels.contains(String(localized: "No remote covers found"))
                == (remoteItems == .linked([]))
        )
        #expect(
            labels.contains(String(localized: "No linked release"))
                == (remoteItems == .unlinked)
        )
        if case .failed = remoteItems {
            #expect(labels.contains("Artwork lookup failed"))
        }
        if case .loading = remoteItems {
            #expect(labels.contains(String(localized: "Fetching covers...")))
        }
    }

    @Test(
        "An unlinked candidate offers identification while retaining release files"
    )
    func identifyFromPicker() async throws {
        var identified = false
        var saved = false
        let size = NSSize(width: 960, height: 700)
        let (window, host) = SnapshotTestSupport.hostInWindow(
            CoverPickerView(
                remoteCoverArts: [],
                localArtwork: PreviewData.bridgeCandidateFiles.images,
                selectedCover: nil,
                fetchRemoteCovers: { .unlinked },
                onFindRelease: { identified = true },
                onSelect: { _ in saved = true },
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
        let observations = try await text(in: host, size: size)
        let labels = observations.compactMap {
            $0.topCandidates(1).first?.string
        }
        #expect(labels.contains(String(localized: "Release Files")))
        #expect(!labels.contains(String(localized: "Refresh")))
        let buttonLabel = String(localized: "Find release…")
            .replacingOccurrences(of: "…", with: "...")
        let button = try #require(
            observations.first {
                $0.topCandidates(1).first?.string
                    .replacingOccurrences(of: "…", with: "...") == buttonLabel
            }
        )
        let point = NSPoint(
            x: button.boundingBox.midX * size.width,
            y: button.boundingBox.midY * size.height
        )
        for type in [NSEvent.EventType.leftMouseDown, .leftMouseUp] {
            window.sendEvent(
                try #require(
                    NSEvent.mouseEvent(
                        with: type,
                        location: point,
                        modifierFlags: [],
                        timestamp: ProcessInfo.processInfo.systemUptime,
                        windowNumber: window.windowNumber,
                        context: nil,
                        eventNumber: 0,
                        clickCount: 1,
                        pressure: type == .leftMouseDown ? 1 : 0
                    )
                )
            )
        }
        await SnapshotTestSupport.settle(host)
        #expect(identified)
        #expect(!saved)
    }

    private func text(in host: NSView, size: NSSize) async throws
        -> [VNRecognizedTextObservation]
    {
        await SnapshotTestSupport.settle(host)
        let png = try await SnapshotTestSupport.capturePNG(host, size: size)
        let request = VNRecognizeTextRequest()
        request.recognitionLevel = .accurate
        try VNImageRequestHandler(data: png, options: [:]).perform([request])
        return try #require(request.results)
    }
}
