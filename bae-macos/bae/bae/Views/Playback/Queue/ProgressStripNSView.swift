import AppKit
import BaeKit
import Combine
import SwiftUI

/// A compact, display-only playback readout: a slim accent bar plus an elapsed
/// label, updated imperatively from the playback-position Combine stream for
/// the same reason as `SeekBarNSView` — position ticks arrive at display rate,
/// far too frequent for SwiftUI observation. Unlike the seek bar it takes no
/// input: the queue panel's now-playing card shows where playback is; seeking
/// stays with the transport bar.
final class ProgressStripNSView: NSView {
    private let bar = ProgressTrackNSView()
    private let elapsedField: NSTextField

    private var positionMs: Int64 = 0
    private var durationMs: UInt64?

    init() {
        elapsedField = NSTextField(labelWithString: "")
        elapsedField.font = .monospacedDigitSystemFont(
            ofSize: 10,
            weight: .semibold
        )
        elapsedField.textColor = .secondaryLabelColor
        elapsedField.alignment = .right
        elapsedField.translatesAutoresizingMaskIntoConstraints = false
        bar.translatesAutoresizingMaskIntoConstraints = false

        super.init(frame: .zero)

        let stack = NSStackView(views: [bar, elapsedField])
        stack.orientation = .horizontal
        stack.spacing = 8
        stack.translatesAutoresizingMaskIntoConstraints = false
        addSubview(stack)

        NSLayoutConstraint.activate([
            stack.leadingAnchor.constraint(equalTo: leadingAnchor),
            stack.trailingAnchor.constraint(equalTo: trailingAnchor),
            stack.centerYAnchor.constraint(equalTo: centerYAnchor),
            bar.heightAnchor.constraint(
                equalToConstant: ProgressTrackNSView.trackHeight
            ),
            // Fixed label slot so the bar doesn't resize as digits change.
            elapsedField.widthAnchor.constraint(equalToConstant: 34),
        ])
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError()
    }

    // MARK: - Direct position updates (called from a Combine subscription)

    func setPosition(progress: Double, positionMs: Int64, durationMs: UInt64) {
        self.positionMs = positionMs
        self.durationMs = durationMs
        bar.progress = progress
        updateLabel()
    }

    /// Clears everything: bar to 0, label empty, duration dropped.
    func reset() {
        bar.progress = 0
        positionMs = 0
        durationMs = nil
        elapsedField.stringValue = ""
    }

    private func updateLabel() {
        guard let durationMs else {
            elapsedField.stringValue = ""
            return
        }
        elapsedField.stringValue =
            DurationClock.seekBar(
                positionMs: positionMs,
                durationMs: durationMs,
                showRemaining: false
            )
            .leading
    }
}

// MARK: - SwiftUI bridge

struct ProgressStripRepresentable: NSViewRepresentable {
    @Environment(\.playbackPositionPublisher)
    private var positionPublisher

    func makeCoordinator() -> Coordinator {
        Coordinator()
    }

    func makeNSView(context: Context) -> ProgressStripNSView {
        let view = ProgressStripNSView()
        context.coordinator.subscribe(to: positionPublisher, view: view)
        return view
    }

    func updateNSView(_: ProgressStripNSView, context _: Context) {}

    class Coordinator {
        private var cancellable: AnyCancellable?

        @MainActor
        func subscribe(
            to publisher: AnyPublisher<PlaybackPositionEvent, Never>,
            view: ProgressStripNSView
        ) {
            cancellable =
                publisher
                .receive(on: DispatchQueue.main)
                .sink { event in
                    switch event {
                    case .position(
                        let progress,
                        let positionMs,
                        let durationMs
                    ):
                        view.setPosition(
                            progress: progress,
                            positionMs: positionMs,
                            durationMs: durationMs
                        )
                    case .reset:
                        view.reset()
                    }
                }
        }
    }
}
