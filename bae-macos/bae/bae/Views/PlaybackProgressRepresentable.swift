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
    let durationMs: UInt64?
    let onSeek: (Double) -> Void

    func makeCoordinator() -> Coordinator {
        Coordinator()
    }

    func makeNSView(context: Context) -> SeekBarNSView {
        let view = SeekBarNSView(
            showsRemainingTimeToggle: true,
            fixedSliderWidth: 300
        )
        view.onSeek = onSeek
        applyDuration(to: view)
        context.coordinator.subscribe(to: positionPublisher, view: view)
        return view
    }

    func updateNSView(_ view: SeekBarNSView, context _: Context) {
        view.onSeek = onSeek
        applyDuration(to: view)
    }

    private func applyDuration(to view: SeekBarNSView) {
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
                    case .position(let progress, let elapsed, let remaining):
                        view.setPosition(
                            progress: progress,
                            elapsed: elapsed,
                            remaining: remaining
                        )
                    case .reset:
                        view.setPosition(
                            progress: 0,
                            elapsed: "",
                            remaining: ""
                        )
                    }
                }
        }
    }
}
