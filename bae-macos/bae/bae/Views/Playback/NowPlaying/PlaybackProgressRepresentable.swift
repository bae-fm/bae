import AppKit
import BaeKit
import Combine
import SwiftUI

// MARK: - Environment key

extension EnvironmentValues {
    @Entry
    var playbackPositionPublisher: AnyPublisher<PlaybackPositionEvent, Never> =
        Empty()
        .eraseToAnyPublisher()
}

// MARK: - SwiftUI bridge

struct PlaybackProgressRepresentable: NSViewRepresentable {
    @Environment(\.playbackPositionPublisher)
    private var positionPublisher
    /// The user's elapsed-vs-remaining choice, read off the config mirror. The
    /// bar keeps no copy of it: clicking the label writes the config, and the
    /// config subscription lands the new value back here.
    let showRemainingTime: Bool
    let durationMs: UInt64?
    let onSeek: (Double) -> Void
    let onToggleRemainingTime: () -> Void

    func makeCoordinator() -> Coordinator {
        Coordinator()
    }

    func makeNSView(context: Context) -> SeekBarNSView {
        let view = SeekBarNSView(
            showsRemainingTimeToggle: true,
            fixedSliderWidth: nil
        )
        apply(to: view)
        context.coordinator.subscribe(to: positionPublisher, view: view)
        return view
    }

    func updateNSView(_ view: SeekBarNSView, context _: Context) {
        apply(to: view)
    }

    private func apply(to view: SeekBarNSView) {
        view.onSeek = onSeek
        view.onToggleRemainingTime = onToggleRemainingTime
        view.showRemainingTime = showRemainingTime
        if let durationMs {
            view.setDuration(durationMs: durationMs)
        }
        else {
            view.clearDuration()
        }
    }

    class Coordinator {
        private var cancellable: AnyCancellable?

        @MainActor
        func subscribe(
            to publisher: AnyPublisher<PlaybackPositionEvent, Never>,
            view: SeekBarNSView
        ) {
            cancellable =
                publisher
                .receive(on: DispatchQueue.main)
                .sink { event in
                    switch event {
                    case .position(let progress, let positionMs, _):
                        view.setPosition(
                            progress: progress,
                            positionMs: positionMs
                        )
                    case .reset:
                        view.reset()
                    }
                }
        }
    }
}
