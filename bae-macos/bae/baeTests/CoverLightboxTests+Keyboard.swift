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

    /// The names of the elements in `window` that `kept` accepts, as the
    /// accessibility tree names them.
    func names(
        in window: NSWindow,
        where kept: (AccessibilityTree.Node) -> Bool
    ) throws -> Set<String> {
        let tree = try AccessibilityTree.read(
            of: window,
            subject: "the cover gallery"
        )
        return Set(
            tree.nodes.filter(kept).compactMap(\.controlName)
        )
    }

    /// The names of the elements in `window` that `galleryOnly` lists.
    func galleryControls(in window: NSWindow) throws -> Set<String> {
        try names(in: window) {
            $0.controlName.map(Self.galleryOnly.contains) ?? false
        }
    }

    /// The names of the elements in `window` that have the keyboard focus.
    func focusedNames(in window: NSWindow) throws -> Set<String> {
        try names(in: window) { $0.focused == true }
    }

    /// Hosts the gallery with two remote covers, its images read, and runs
    /// `body` on its window.
    func withGallery(
        _ body: (NSWindow, NSView, CoverItem, LightboxImageLoad) async throws
            -> Void
    ) async throws {
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
            try await body(window, host, front, lightbox)
        }
    }

    /// What has the keyboard focus after each of `steps` presses of Tab:
    /// each element's role and name, `unnamed` for an element with none.
    func tabWalk(
        in window: NSWindow,
        host: NSView,
        steps: Int
    ) async throws -> [[String]] {
        var walk: [[String]] = []
        for _ in 0..<steps {
            HostedInput.selectNextKey(in: window)
            try await SnapshotTestSupport.settle(host)
            let tree = try AccessibilityTree.read(
                of: window,
                subject: "the cover gallery"
            )
            walk.append(
                tree.nodes.filter { $0.focused == true }
                    .map { "\($0.role) \($0.controlName ?? "unnamed")" }
            )
        }
        return walk
    }

    @Test("With the lightbox closed, Tab stops only at named controls")
    func closedTabStopsAreNamed() async throws {
        try await withGallery { window, host, _, _ in
            let walk = try await tabWalk(in: window, host: host, steps: 24)
            print("TABWALK", walk)
            #expect(!walk.joined().contains { $0.hasSuffix(" unnamed") })
        }
    }

    @Test(
        "With the lightbox open, the keyboard and VoiceOver reach only the lightbox"
    )
    func lightboxTakesTheKeyboard() async throws {
        try await withGallery { window, host, front, lightbox in
            // Closed, the gallery's controls are in the tree.
            #expect(
                try galleryControls(in: window)
                    .isSuperset(of: ["Use this cover", "Done"])
            )
            try open(.space, window: window, host: host)
            try await Wait.until { lightbox.ended(loading: front.image) }
            try await SnapshotTestSupport.settle(host)
            // Open, VoiceOver reads none of them.
            #expect(try galleryControls(in: window).isEmpty)
            // Tab goes round the lightbox's own controls and never to one
            // of the gallery's, and never to nothing the tree lists.
            let walk = try await tabWalk(in: window, host: host, steps: 16)
            for focused in walk {
                #expect(!focused.isEmpty)
                #expect(
                    focused.allSatisfy { stop in
                        !Self.galleryOnly.contains { stop.hasSuffix($0) }
                    }
                )
            }
        }
    }
}
