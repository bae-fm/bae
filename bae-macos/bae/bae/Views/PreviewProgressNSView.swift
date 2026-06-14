import AppKit
import Combine
import SwiftUI

// MARK: - Event type

enum PreviewProgressEvent {
    case position(progress: Double, elapsed: String)
    case reset
}

/// AppKit view for the preview progress bar + time labels.
///
/// Updated directly from the reducer, bypassing SwiftUI observation entirely.
/// Same pattern as PlaybackProgressNSView — avoids SwiftUI re-renders on every tick.
class PreviewProgressNSView: NSView {
    private let elapsedField: NSTextField
    private let slider: SeekSlider
    private let durationField: NSTextField

    var onSeek: ((Double) -> Void)?
    var durationMs: UInt64?

    private var currentElapsed = ""

    override init(frame: NSRect) {
        let font = NSFont.monospacedDigitSystemFont(
            ofSize: 10,
            weight: .regular
        )
        let color = NSColor.secondaryLabelColor

        elapsedField = NSTextField(labelWithString: "")
        elapsedField.font = font
        elapsedField.textColor = color
        elapsedField.alignment = .right
        elapsedField.translatesAutoresizingMaskIntoConstraints = false

        slider = SeekSlider()
        slider.minValue = 0
        slider.maxValue = 1
        slider.isContinuous = true
        slider.translatesAutoresizingMaskIntoConstraints = false

        durationField = NSTextField(labelWithString: "")
        durationField.font = font
        durationField.textColor = color
        durationField.alignment = .left
        durationField.translatesAutoresizingMaskIntoConstraints = false

        super.init(frame: frame)

        slider.target = self
        slider.action = #selector(sliderValueChanged(_:))
        slider.onSeekComplete = { [weak self] value in
            self?.seekCompleted(value)
        }

        let stack = NSStackView(views: [elapsedField, slider, durationField])
        stack.orientation = .horizontal
        stack.spacing = 8
        stack.translatesAutoresizingMaskIntoConstraints = false
        addSubview(stack)

        NSLayoutConstraint.activate([
            stack.leadingAnchor.constraint(equalTo: leadingAnchor),
            stack.trailingAnchor.constraint(equalTo: trailingAnchor),
            stack.centerYAnchor.constraint(equalTo: centerYAnchor),
            elapsedField.widthAnchor.constraint(equalToConstant: 40),
            durationField.widthAnchor.constraint(equalToConstant: 40),
        ])
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError()
    }

    // MARK: - Direct position updates (called from reducer, not SwiftUI)

    func setPosition(progress: Double, elapsed: String) {
        currentElapsed = elapsed

        if !slider.isDragging {
            slider.doubleValue = progress
            elapsedField.stringValue = currentElapsed
        }
    }

    func setDuration(_ label: String, durationMs: UInt64) {
        self.durationMs = durationMs
        durationField.stringValue = label
    }

    func reset() {
        slider.doubleValue = 0
        elapsedField.stringValue = ""
        durationField.stringValue = ""
        durationMs = nil
        currentElapsed = ""
    }

    // MARK: - Internal

    @objc
    private func sliderValueChanged(_: SeekSlider) {
        guard slider.isDragging, let durationMs, durationMs > 0 else {
            return
        }
        elapsedField.stringValue = formatTimeAtRatio(
            ratio: slider.doubleValue,
            durationMs: durationMs
        )
    }

    private func seekCompleted(_ value: Double) {
        onSeek?(value)
        elapsedField.stringValue = currentElapsed
    }
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
    let durationLabel: String
    let onSeek: (Double) -> Void

    func makeCoordinator() -> Coordinator {
        Coordinator()
    }

    func makeNSView(context: Context) -> PreviewProgressNSView {
        let view = PreviewProgressNSView()
        view.onSeek = onSeek
        view.setDuration(durationLabel, durationMs: durationMs)
        context.coordinator.subscribe(to: publisher, view: view)
        return view
    }

    func updateNSView(_ view: PreviewProgressNSView, context _: Context) {
        view.onSeek = onSeek
        view.setDuration(durationLabel, durationMs: durationMs)
    }

    class Coordinator {
        private var cancellable: AnyCancellable?

        @MainActor
        func subscribe(
            to publisher: AnyPublisher<PreviewProgressEvent, Never>,
            view: PreviewProgressNSView
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
