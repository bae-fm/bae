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
        try await SnapshotTestSupport.withHostedWindow(
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
        ) { _, host in
            let observations = try await text(in: host, size: size)
            let labels = observations.map(\.text)
            #expect(
                labels.carrying(String(localized: "No remote covers found"))
                    == (remoteItems == .linked([]))
            )
            #expect(
                labels.carrying(String(localized: "No linked release"))
                    == (remoteItems == .unlinked)
            )
            if case .failed = remoteItems {
                #expect(labels.carrying("Artwork lookup failed"))
            }
            if case .loading = remoteItems {
                #expect(
                    labels.carrying(String(localized: "Fetching covers..."))
                )
            }
        }
    }

    @Test(
        "An unlinked candidate offers identification while retaining release files"
    )
    func identifyFromPicker() async throws {
        var identified = false
        var saved = false
        let size = NSSize(width: 960, height: 700)
        try await SnapshotTestSupport.withHostedWindow(
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
        ) { window, host in
            let observations = try await text(in: host, size: size)
            let labels = observations.map(\.text)
            #expect(labels.carrying(String(localized: "Release Files")))
            #expect(!labels.carrying(String(localized: "Refresh")))
            let buttonLabel = String(localized: "Find release…")
                .replacingOccurrences(of: "…", with: "...")
            let button = try #require(
                observations.first {
                    $0.text
                        .replacingOccurrences(of: "…", with: "...")
                        .contains(buttonLabel)
                }
            )
            let point = NSPoint(
                x: button.boundingBox.midX * size.width,
                y: button.boundingBox.midY * size.height
            )
            try HostedInput.click(at: point, in: window)
            try await SnapshotTestSupport.settle(host)
            #expect(identified)
            #expect(!saved)
        }
    }

    private func text(in host: NSView, size: NSSize) async throws
        -> [SnapshotTestSupport.RecognizedLine]
    {
        try await SnapshotTestSupport.settle(host)
        let png = try await SnapshotTestSupport.capturePNG(host, size: size)
        return try await SnapshotTestSupport.recognizedText(in: png)
    }
}
