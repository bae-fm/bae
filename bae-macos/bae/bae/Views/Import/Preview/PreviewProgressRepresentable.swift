import AppKit
import BaeKit
import Combine
import SwiftUI

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
                    case .position(let progress, let positionMs):
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

#if DEBUG
    #Preview("Preview progress bar") {
        PreviewProgressRepresentable(
            durationMs: 195_000,
            onSeek: { _ in },
        )
        .environment(
            \.previewProgressPublisher,
            Just(
                PreviewProgressEvent.position(progress: 0.4, positionMs: 78_000)
            )
            .eraseToAnyPublisher()
        )
        .frame(width: 320, height: 40)
        .padding()
    }
#endif
