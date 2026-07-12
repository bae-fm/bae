import AppKit
import BaeKit
import Combine
import SwiftUI

// MARK: - Event type

enum PreviewProgressEvent {
    case position(progress: Double, elapsed: String)
    case reset
}

// MARK: - Environment key

extension EnvironmentValues {
    @Entry
    var previewProgressPublisher: AnyPublisher<PreviewProgressEvent, Never> =
        Empty()
        .eraseToAnyPublisher()
}

// MARK: - SwiftUI bridge

struct PreviewProgressRepresentable: NSViewRepresentable {
    @Environment(\.previewProgressPublisher)
    private var publisher
    let durationMs: UInt64
    let onSeek: (Double) -> Void

    func makeCoordinator() -> Coordinator {
        Coordinator()
    }

    func makeNSView(context: Context) -> SeekBarNSView {
        let view = SeekBarNSView(
            showsRemainingTimeToggle: false,
            fixedSliderWidth: nil
        )
        view.onSeek = onSeek
        view.setDuration(durationMs: durationMs)
        context.coordinator.subscribe(to: publisher, view: view)
        return view
    }

    func updateNSView(_ view: SeekBarNSView, context _: Context) {
        view.onSeek = onSeek
        view.setDuration(durationMs: durationMs)
    }

    class Coordinator {
        private var cancellable: AnyCancellable?

        @MainActor
        func subscribe(
            to publisher: AnyPublisher<PreviewProgressEvent, Never>,
            view: SeekBarNSView
        ) {
            cancellable =
                publisher
                .receive(on: DispatchQueue.main)
                .sink { event in
                    switch event {
                    case .position(let progress, let elapsed):
                        view.setPosition(progress: progress, elapsed: elapsed)
                    case .reset:
                        view.reset()
                    }
                }
        }
    }
}
