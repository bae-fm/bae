import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

extension CoverLightboxTests {
    /// The controls only the gallery has: the lightbox has none of them.
    static let galleryOnly: Set<String> = [
        "Use this cover", "Done", "View in lightbox",
        "Previous image", "Next image",
    ]

    /// The names of the elements in `window` that `galleryOnly` lists.
    func galleryControls(in window: NSWindow) throws -> Set<String> {
        let tree = try AccessibilityTree.read(
            of: window,
            subject: "the cover gallery"
        )
        return Set(
            tree.nodes.compactMap { $0.title ?? $0.description }
                .filter(Self.galleryOnly.contains)
        )
    }

    /// The names of the elements in `window` that have the keyboard focus.
    func focusedNames(in window: NSWindow) throws -> Set<String> {
        let tree = try AccessibilityTree.read(
            of: window,
            subject: "the cover gallery"
        )
        return Set(
            tree.nodes.filter { $0.focused == true }
                .compactMap { $0.title ?? $0.description }
        )
    }

    @Test(
        "With the lightbox open, the keyboard and VoiceOver reach only the lightbox"
    )
    func lightboxTakesTheKeyboard() async throws {
        let front = remote("Front")
        let booklet = remote("Booklet")
        let bytes = try Data(
            contentsOf: URL(
                fileURLWithPath: PreviewData.previewArtPath("Lightbox fixture")
            )
        )
        let recorder = Reads()
        let images = imageStore(serving: bytes, recording: recorder)
        let lightbox = LightboxImageLoad()
        let size = NSSize(width: 960, height: 700)
        try await SnapshotTestSupport.withHostedWindow(
            FocusingCoverGallery(
                gallery: gallery(remote: [front, booklet], files: [])
            ) { content, focusedCover in
                MacCoverGallery(
                    content: content,
                    remoteItems: .linked([front, booklet]),
                    filter: .constant(.all),
                    focusedCover: focusedCover,
                    selectedCover: front.selection,
                    lightboxLoad: lightbox,
                    onSelect: { _ in },
                    onDone: {}
                )
            }
            .environment(images)
            .frame(width: size.width, height: size.height),
            size: size
        ) { window, host in
            try await Wait.until {
                Set(await recorder.reads.map(\.url)).count >= 2
            }
            try await SnapshotTestSupport.settle(host)
            // Closed, the gallery's controls are in the tree.
            #expect(
                try galleryControls(in: window)
                    .isSuperset(of: ["Use this cover", "Done"])
            )
            // Tab moves the keyboard focus about the gallery, so the walk
            // below can tell where it goes.
            var closed: Set<String> = []
            for _ in 0..<16 {
                HostedInput.selectNextKey(in: window)
                try await SnapshotTestSupport.settle(host)
                closed.formUnion(try focusedNames(in: window))
            }
            #expect(!closed.isEmpty)
            try open(.space, window: window, host: host)
            try await Wait.until { lightbox.ended(loading: front.image) }
            try await SnapshotTestSupport.settle(host)
            // Open, VoiceOver reads none of them.
            #expect(try galleryControls(in: window).isEmpty)
            // Tab goes round the lightbox's own controls and never to one
            // of the gallery's.
            for _ in 0..<16 {
                HostedInput.selectNextKey(in: window)
                try await SnapshotTestSupport.settle(host)
                let focused = try focusedNames(in: window)
                // The focus stays on the lightbox's own controls: not on the
                // gallery's, and not on nothing the tree lists.
                #expect(!focused.isEmpty)
                #expect(focused.isDisjoint(with: Self.galleryOnly))
            }
        }
    }
}
