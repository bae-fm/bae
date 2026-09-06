import AppKit
import BaeKit
import Combine
import SwiftUI

/// Both seek bars render their entire timeline from one subscription.
/// SwiftUI updates appearance, preferences, and callbacks only.
struct SeekBarRepresentable: NSViewRepresentable {
    @Environment(\.accentChoice)
    private var accent
    @Environment(\.colorScheme)
    private var colorScheme

    let positions: AnyPublisher<PlaybackPositionEvent, Never>
    let showRemainingTime: Bool
    let onSeek: (Double) -> Void
    let onToggleRemainingTime: (() -> Void)?

    func makeCoordinator() -> Coordinator {
        Coordinator()
    }

    func makeNSView(context: Context) -> SeekBarNSView {
        let view = SeekBarNSView(
            showsRemainingTimeToggle: onToggleRemainingTime != nil
        )
        apply(to: view)
        context.coordinator.subscribe(to: positions, view: view)
        return view
    }

    func updateNSView(_ view: SeekBarNSView, context _: Context) {
        apply(to: view)
    }

    private func apply(to view: SeekBarNSView) {
        view.accent = NSColor(accent.color(in: colorScheme))
        view.onSeek = onSeek
        view.onToggleRemainingTime = onToggleRemainingTime
        view.showRemainingTime = showRemainingTime
    }

    final class Coordinator {
        private var cancellable: AnyCancellable?

        @MainActor
        func subscribe(
            to publisher: AnyPublisher<PlaybackPositionEvent, Never>,
            view: SeekBarNSView
        ) {
            cancellable =
                publisher
                .receive(on: DispatchQueue.main)
                .sink { view.apply($0) }
        }
    }
}
