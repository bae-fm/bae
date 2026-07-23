import AppKit
import Combine
import SwiftUI

// MARK: - Event type

/// Loudness-measurement tick for an importing candidate. `key` routes it to that
/// candidate's confirm pane; `fraction` (0...1) drives the determinate bar as the
/// scan creeps through each track, and `tracksDone`/`tracksTotal` label "N / M".
struct ImportLoudnessProgressEvent {
    let key: String
    let tracksDone: UInt32
    let tracksTotal: UInt32
    let fraction: Double
}

/// AppKit view for the loudness-measurement bar shown during an import.
///
/// Updated directly from `DesktopUiEvents`' Combine signal, bypassing SwiftUI
/// observation entirely — same pattern as `SeekBarNSView`, so the
/// high-frequency sub-track ticks never re-render the confirm pane tree. The
/// label is the localized `ui.import.loudness_progress` line ("Measuring
/// loudness — N/M"); the bar is the overall scan `fraction`.
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

    // MARK: - Direct updates (called from DesktopUiEvents, not SwiftUI)

    func setProgress(tracksDone: UInt32, tracksTotal: UInt32, fraction: Double)
    {
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
        bar.doubleValue = fraction
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
