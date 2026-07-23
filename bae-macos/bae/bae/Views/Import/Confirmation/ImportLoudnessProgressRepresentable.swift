import AppKit
import Combine
import SwiftUI

// MARK: - Environment key

extension EnvironmentValues {
    @Entry
    var importLoudnessPublisher:
        AnyPublisher<ImportLoudnessProgressEvent?, Never> =
            Empty()
            .eraseToAnyPublisher()
}

// MARK: - SwiftUI bridge

/// Leaf view for one candidate's loudness progress. Subscribes to the shared
/// signal and renders only the ticks whose key matches this candidate.
struct ImportLoudnessProgressRepresentable: NSViewRepresentable {
    @Environment(\.importLoudnessPublisher)
    private var publisher
    let key: String

    func makeCoordinator() -> Coordinator {
        Coordinator(key: key)
    }

    func makeNSView(context: Context) -> ImportLoudnessProgressNSView {
        let view = ImportLoudnessProgressNSView()
        context.coordinator.subscribe(to: publisher, view: view)
        return view
    }

    func updateNSView(_: ImportLoudnessProgressNSView, context _: Context) {}

    class Coordinator {
        private let key: String
        private var cancellable: AnyCancellable?

        init(key: String) {
            self.key = key
        }

        @MainActor
        func subscribe(
            to publisher: AnyPublisher<ImportLoudnessProgressEvent?, Never>,
            view: ImportLoudnessProgressNSView
        ) {
            let key = key
            cancellable =
                publisher
                .compactMap { $0 }
                .filter { $0.key == key }
                .receive(on: DispatchQueue.main)
                .sink { event in
                    view.setProgress(
                        tracksDone: event.tracksDone,
                        tracksTotal: event.tracksTotal,
                        fraction: event.fraction
                    )
                }
        }
    }
}

#if DEBUG
    #Preview("Loudness progress") {
        ImportLoudnessProgressRepresentable(key: "preview-candidate")
            .environment(
                \.importLoudnessPublisher,
                Just(
                    ImportLoudnessProgressEvent(
                        key: "preview-candidate",
                        tracksDone: 3,
                        tracksTotal: 9,
                        fraction: 0.34
                    )
                )
                .eraseToAnyPublisher()
            )
            .frame(width: 220, height: 44)
            .padding()
    }
#endif

/// Loudness-measurement tick for an importing candidate. `key` routes it to that
/// candidate's confirm pane; `fraction` (0...1) drives the determinate bar as the
/// scan creeps through each track, and `tracksDone`/`tracksTotal` label "N / M".
struct ImportLoudnessProgressEvent {
    let key: String
    let tracksDone: UInt32
    let tracksTotal: UInt32
    let fraction: Double
}
