import AppKit
import BaeKit
import Combine
import SwiftUI
import Testing

@testable import bae

private struct ProbeKey: EnvironmentKey {
    static let defaultValue = "unset"
}

extension EnvironmentValues {
    fileprivate var probe: String {
        get { self[ProbeKey.self] }
        set { self[ProbeKey.self] = newValue }
    }
}

/// What the now playing bar's right controls keep when they sit in a hosting
/// view of their own.
@MainActor
@Suite("Now-playing bar hosting")
struct NowPlayingBarHostingTests {
    private final class Seen {
        var value: String?
    }

    private struct Reader: View {
        @Environment(\.probe)
        var probe
        let seen: Seen

        var body: some View {
            seen.value = probe
            return Color.clear.frame(width: 10, height: 10)
        }
    }

    /// Shows `root` in an off-screen window and lays it out.
    private func shown<Root: View>(_ root: Root) -> NSHostingView<Root> {
        let view = NSHostingView(rootView: root)
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 900, height: 120),
            styleMask: [.borderless],
            backing: .buffered,
            defer: true
        )
        window.contentView = view
        for _ in 0..<3 {
            view.layoutSubtreeIfNeeded()
            RunLoop.main.run(until: Date(timeIntervalSinceNow: 0.05))
        }
        return view
    }

    @Test("hosted content reads the environment of the place it sits in")
    func passesTheEnvironment() {
        let seen = Seen()
        _ = shown(
            SeparatelyHosted { Reader(seen: seen) }
                .environment(\.probe, "from the bar")
        )
        #expect(seen.value == "from the bar")
    }

    @Test("the queue button's drop target survives the separate hosting view")
    func queueDropTargetRegisters() {
        let view = shown(
            MacNowPlayingBar(
                trackTitle: "Track Title",
                secondaryLine: "Artist Name",
                cover: nil,
                isPlaying: true,
                position: nil,
                buffered: nil,
                waitsOnStorage: false,
                showRemainingTime: false,
                settings: nil,
                receiverWithoutVolume: nil,
                shuffled: false,
                showQueue: false,
                onPlayPause: {},
                onNext: {},
                onPrevious: {},
                onSeek: { _ in },
                onToggleRemainingTime: {},
                onVolumeDrag: { _ in },
                onVolumeChange: { _ in },
                onSetMuted: { _ in },
                onSetShuffle: { _ in },
                onSetQueuePresented: { _ in },
                onSetRepeatMode: { _ in },
                onDropToQueue: { _ in },
                onNavigateToAlbum: {},
                queueAddPublisher: Empty().eraseToAnyPublisher(),
                castControl: AnyView(EmptyView())
            )
            .frame(width: 900, height: 100)
            .environment(ImageStore.stub())
        )
        let hosts = Self.hostingViews(in: view).filter { $0 !== view }
        let tree = Self.describe(view, depth: 0)
        #expect(!hosts.isEmpty, "\(tree)")
        #expect(
            hosts.contains { !$0.registeredDraggedTypes.isEmpty },
            "\(tree)"
        )
    }

    private static func describe(_ view: NSView, depth: Int) -> String {
        let types = view.registeredDraggedTypes.map(\.rawValue)
        var text =
            String(repeating: " ", count: depth * 2)
            + "\(type(of: view)) \(types)\n"
        for sub in view.subviews {
            text += describe(sub, depth: depth + 1)
        }
        return text
    }

    private static func hostingViews(in view: NSView) -> [NSView] {
        var found: [NSView] = []
        if String(describing: type(of: view)).contains("NSHostingView") {
            found.append(view)
        }
        for sub in view.subviews { found += hostingViews(in: sub) }
        return found
    }
}
