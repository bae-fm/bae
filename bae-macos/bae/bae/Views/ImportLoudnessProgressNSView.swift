import AppKit
import Combine
import SwiftUI

// MARK: - Event type

/// Per-track loudness measurement tick for an importing candidate. `key` routes
/// it to that candidate's confirm pane; `tracksDone`/`tracksTotal` drive the
/// determinate bar and the "N / M" label.
struct ImportLoudnessProgressEvent {
    let key: String
    let tracksDone: UInt32
    let tracksTotal: UInt32
}

/// AppKit view for the loudness-measurement bar shown during an import.
///
/// Updated directly from the reducer's Combine signal, bypassing SwiftUI
/// observation entirely — same pattern as `PreviewProgressNSView`, so the
/// one-per-track ticks never re-render the confirm pane tree. The label is the
/// localized `ui.import.loudness_progress` line ("Measuring loudness — N/M");
/// the bar is its determinate ratio.
class ImportLoudnessProgressNSView: NSView {
    private let label: NSTextField
    private let bar: NSProgressIndicator

    override init(frame: NSRect) {
        label = NSTextField(labelWithString: "")
        label.font = .systemFont(ofSize: 11)
        label.textColor = .secondaryLabelColor
        label.translatesAutoresizingMaskIntoConstraints = false

        bar = NSProgressIndicator()
        bar.isIndeterminate = false
        bar.minValue = 0
        bar.maxValue = 1
        bar.doubleValue = 0
        bar.controlSize = .small
        bar.translatesAutoresizingMaskIntoConstraints = false

        super.init(frame: frame)

        let stack = NSStackView(views: [label, bar])
        stack.orientation = .vertical
        stack.alignment = .leading
        stack.spacing = 4
        stack.translatesAutoresizingMaskIntoConstraints = false
        addSubview(stack)

        NSLayoutConstraint.activate([
            stack.leadingAnchor.constraint(equalTo: leadingAnchor),
            stack.trailingAnchor.constraint(equalTo: trailingAnchor),
            stack.topAnchor.constraint(equalTo: topAnchor),
            stack.bottomAnchor.constraint(equalTo: bottomAnchor),
            bar.widthAnchor.constraint(equalToConstant: 200),
        ])
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError()
    }

    // MARK: - Direct updates (called from the reducer signal, not SwiftUI)

    func setProgress(tracksDone: UInt32, tracksTotal: UInt32) {
        label.stringValue = String(
            format: NSLocalizedString(
                "ui.import.loudness_progress",
                tableName: "Core",
                bundle: .main,
                comment: ""
            ),
            Int(tracksDone),
            Int(tracksTotal)
        )
        bar.doubleValue =
            tracksTotal == 0 ? 0 : Double(tracksDone) / Double(tracksTotal)
    }
}

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
                .receive(on: DispatchQueue.main)
                .sink { event in
                    guard let event, event.key == key else { return }
                    view.setProgress(
                        tracksDone: event.tracksDone,
                        tracksTotal: event.tracksTotal
                    )
                }
        }
    }
}
