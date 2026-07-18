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
    private let bar = ProgressBarView()
    private let elapsedField: NSTextField

    private var positionMs: UInt64 = 0
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
            bar.heightAnchor.constraint(equalToConstant: 4),
            // Fixed label slot so the bar doesn't resize as digits change.
            elapsedField.widthAnchor.constraint(equalToConstant: 34),
        ])
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError()
    }

    // MARK: - Direct position updates (called from a Combine subscription)

    func setPosition(progress: Double, positionMs: UInt64, durationMs: UInt64) {
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

/// The strip's bar: a rounded track with an accent fill up to `progress`.
private final class ProgressBarView: NSView {
    var progress: Double = 0 {
        didSet {
            if progress != oldValue {
                needsDisplay = true
            }
        }
    }

    override func draw(_: NSRect) {
        let radius = bounds.height / 2
        NSColor.white.withAlphaComponent(0.12).setFill()
        NSBezierPath(roundedRect: bounds, xRadius: radius, yRadius: radius)
            .fill()

        let fillWidth = bounds.width * CGFloat(min(max(progress, 0), 1))
        guard fillWidth > 0 else {
            return
        }
        let fillRect = NSRect(
            x: 0,
            y: 0,
            width: max(fillWidth, bounds.height),
            height: bounds.height
        )
        let accent = NSColor(Theme.accent)
        let lighter =
            accent.blended(withFraction: 0.25, of: .white) ?? accent
        let path = NSBezierPath(
            roundedRect: fillRect,
            xRadius: radius,
            yRadius: radius
        )
        NSGradient(starting: accent, ending: lighter)?
            .draw(in: path, angle: 0)
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
