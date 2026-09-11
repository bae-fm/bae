import AppKit
import SwiftUI
import Testing
import Vision

/// Shared AppKit hosting + snapshot helpers for the view tests.
enum SnapshotTestSupport {
    /// Host `view` (sized to `size`) in a borderless key window. The caller keeps
    /// the returned window alive for the test's duration and uses the host to
    /// capture pixels or send events through the window.
    @MainActor
    static func hostInWindow<V: View>(
        _ view: V,
        size: NSSize
    ) -> (window: NSWindow, host: NSHostingView<V>) {
        let bounds = NSRect(origin: .zero, size: size)
        let host = NSHostingView(rootView: view)
        host.frame = bounds
        let window = SnapshotTestWindow(
            contentRect: bounds,
            styleMask: [.borderless],
            backing: .buffered,
            defer: false
        )
        window.contentView = host
        window.makeKeyAndOrderFront(nil)
        return (window, host)
    }

    /// Lay out `host` and capture it as PNG bytes. Yields once so SwiftUI's
    /// async work settles, and sleeps `waitNanoseconds` first when the view has
    /// async content (a cover load) that must resolve before the capture.
    @MainActor
    static func capturePNG(
        _ host: NSView,
        size: NSSize,
        waitNanoseconds: UInt64 = 0
    ) async throws -> Data {
        let bounds = NSRect(origin: .zero, size: size)
        host.layoutSubtreeIfNeeded()
        await Task.yield()
        if waitNanoseconds > 0 {
            try await Task.sleep(nanoseconds: waitNanoseconds)
        }
        host.layoutSubtreeIfNeeded()
        let bitmap = try #require(
            host.bitmapImageRepForCachingDisplay(in: bounds)
        )
        host.cacheDisplay(in: bounds, to: bitmap)
        return try #require(bitmap.representation(using: .png, properties: [:]))
    }

    /// Let SwiftUI publish its renders before a hosted-view test inspects or
    /// interacts with it: yield until a turn changes no frame in the hosted
    /// tree.
    ///
    /// SwiftUI lays a hosted tree out over several main-actor turns — a
    /// geometry reader publishes a width, the views under it re-measure, a
    /// text wraps — and how many turns that takes depends on the machine. A
    /// fixed number of yields measured a tree mid-layout on a slow runner and
    /// gave frames a few points off. Convergence is what "settled" means, so
    /// that is what is waited for, after the floor of turns every hosted
    /// view needs to publish at all. A view that animates never converges;
    /// `maxTurns` bounds the wait and leaves it as the last turn drew it.
    @MainActor
    static func settle(
        _ host: NSView,
        minimumTurns: Int = 3,
        maxTurns: Int = 120
    ) async {
        var previous: [CGRect]?
        for turn in 0..<maxTurns {
            host.layoutSubtreeIfNeeded()
            let frames = descendants(of: host).map(\.frame)
            if turn >= minimumTurns, frames == previous {
                return
            }
            previous = frames
            await Task.yield()
        }
        host.layoutSubtreeIfNeeded()
    }

    /// Every AppKit view below `view`, depth first. SwiftUI controls may be
    /// nested below private hosting containers, so interaction tests use the
    /// full hosted tree rather than assuming one framework-specific depth.
    @MainActor
    static func descendants(of view: NSView) -> [NSView] {
        view.subviews.flatMap { [$0] + descendants(of: $0) }
    }

    /// One line of text Vision read off a capture: the words, and where on
    /// the image they sit (Vision's normalized box, origin at the bottom left).
    /// A value rather than the observation itself, so it can cross the task
    /// that races the recognizer against its deadline.
    struct RecognizedLine: Sendable {
        let text: String
        let boundingBox: CGRect
    }

    /// Vision's recognizer did not answer within the deadline.
    ///
    /// The recognizer runs in-process on a system service, and that service
    /// wedges now and then on this machine: the request parks on a semaphore
    /// that is never signalled and the whole test host sits there until
    /// somebody notices, hours later. A deadline turns that into one failed
    /// test with this error as its reason.
    struct TextRecognitionTimedOut: Error, CustomStringConvertible {
        let after: TimeInterval

        var description: String {
            "Vision text recognition did not answer within \(after)s; "
                + "the recognizer service is wedged, not the view under test"
        }
    }

    /// The lines of text drawn in `png`, read by Vision's accurate recognizer.
    ///
    /// The recognition runs on a dispatch queue and is raced against
    /// `timeout`: whichever finishes first resumes the caller, and the other
    /// finds the continuation already claimed. Dispatch rather than a task
    /// group: a group waits for every child before it returns, and a child
    /// parked inside a wedged recognizer never returns, so a deadline thrown
    /// from a sibling could not end it. A request that has wedged cannot be
    /// unblocked at all; its thread is abandoned, and the test host is a
    /// throwaway process.
    static func recognizedText(
        in png: Data,
        languages: [String]? = nil,
        timeout: TimeInterval = 60
    ) async throws -> [RecognizedLine] {
        let first = FirstToFinish()
        return try await withCheckedThrowingContinuation { continuation in
            DispatchQueue.global(qos: .userInitiated)
                .async {
                    let outcome = Result {
                        try recognize(png, languages: languages)
                    }
                    if first.claim() {
                        continuation.resume(with: outcome)
                    }
                }
            DispatchQueue.global()
                .asyncAfter(deadline: .now() + timeout) {
                    if first.claim() {
                        continuation.resume(
                            throwing: TextRecognitionTimedOut(after: timeout)
                        )
                    }
                }
        }
    }

    private static func recognize(
        _ png: Data,
        languages: [String]?
    ) throws -> [RecognizedLine] {
        let request = VNRecognizeTextRequest()
        request.recognitionLevel = .accurate
        if let languages {
            request.recognitionLanguages = languages
        }
        try VNImageRequestHandler(data: png, options: [:]).perform([request])
        return (request.results ?? [])
            .map { observation in
                RecognizedLine(
                    text: observation.topCandidates(1).first?.string ?? "",
                    boundingBox: observation.boundingBox
                )
            }
    }

    /// Which of two racing tasks gets to resume a continuation: the first to
    /// claim, exactly once.
    private final class FirstToFinish: @unchecked Sendable {
        private let lock = NSLock()
        private var claimed = false

        func claim() -> Bool {
            lock.lock()
            defer { lock.unlock() }
            if claimed {
                return false
            }
            claimed = true
            return true
        }
    }
}

extension Collection<String> {
    /// Whether any of these lines carries `text`.
    ///
    /// Text recognition reads a line as it was drawn, glyphs included: a
    /// section header's symbol, an error's warning triangle, a checkbox's box
    /// — each shares a baseline with its words and comes back glued to them,
    /// and how much of the glyph survives depends on the machine that drew
    /// the pixels. A check that the words were drawn asks whether a line
    /// carries them, not whether one equals them.
    func carrying(_ text: String) -> Bool {
        contains { $0.contains(text) }
    }
}

private final class SnapshotTestWindow: NSWindow {
    override var canBecomeKey: Bool { true }
}
