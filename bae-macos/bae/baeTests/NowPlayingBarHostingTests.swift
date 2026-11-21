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

    /// Shows `root` in an off-screen window and lays it out, twice: the first
    /// pass makes the platform views, the second the views they host.
    private func shown<Root: View>(_ root: Root) -> NSHostingView<Root> {
        let view = NSHostingView(rootView: root)
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 900, height: 120),
            styleMask: [.borderless],
            backing: .buffered,
            defer: true
        )
        window.contentView = view
        // Laying out the hosting view makes its platform views, and laying
        // those out makes the views they host.
        view.layoutSubtreeIfNeeded()
        view.layoutSubtreeIfNeeded()
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
        let names = view.subviews.map { String(describing: type(of: $0)) }
        let seek = names.firstIndex { $0.contains("MacSeekBar") }
        let controls = names.firstIndex { $0.contains("SeparatelyHosted") }
        #expect(seek != nil && controls != nil, "\(names)")
        if let seek, let controls {
            #expect(seek < controls, "\(names)")
            #expect(
                Self.holdsDropTarget(view.subviews[controls]),
                "\(Self.describe(view, depth: 0))"
            )
        }
    }

    private static func holdsDropTarget(_ view: NSView) -> Bool {
        !view.registeredDraggedTypes.isEmpty
            || view.subviews.contains { holdsDropTarget($0) }
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
}
