import AppKit
import Combine
import SwiftUI

/// AppKit view for the playback progress bar + time labels.
///
/// Updated directly from AppService.onPlaybackProgress, bypassing SwiftUI
/// observation entirely. This eliminates the ~15-20% CPU overhead from
/// NSHostingView constraint recalculation on every position tick.
class PlaybackProgressNSView: NSView {
    private let elapsedField: NSTextField
    private let slider: SeekSlider
    private let durationField: NSTextField

    var onSeek: ((Double) -> Void)?

    private var durationMs: UInt64?
    private var currentElapsed = ""
    private var currentRemaining = ""

    private var showRemainingTime: Bool {
        get { UserDefaults.standard.bool(forKey: "showRemainingTime") }
        set { UserDefaults.standard.set(newValue, forKey: "showRemainingTime") }
    }

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

        let click = NSClickGestureRecognizer(
            target: self,
            action: #selector(elapsedTapped)
        )
        elapsedField.addGestureRecognizer(click)

        let stack = NSStackView(views: [elapsedField, slider, durationField])
        stack.orientation = .horizontal
        stack.spacing = 8
        stack.translatesAutoresizingMaskIntoConstraints = false
        addSubview(stack)

        NSLayoutConstraint.activate([
            stack.leadingAnchor.constraint(equalTo: leadingAnchor),
            stack.trailingAnchor.constraint(equalTo: trailingAnchor),
            stack.centerYAnchor.constraint(equalTo: centerYAnchor),
            elapsedField.widthAnchor.constraint(equalToConstant: 48),
            slider.widthAnchor.constraint(equalToConstant: 300),
            durationField.widthAnchor.constraint(equalToConstant: 40),
        ])
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError()
    }

    // MARK: - Direct position updates (called from AppService, not SwiftUI)

    func setPosition(progress: Double, elapsed: String, remaining: String) {
        currentElapsed = elapsed
        currentRemaining = remaining

        if !slider.isDragging {
            slider.doubleValue = progress
            updateElapsedLabel(seekLabel: nil)
        }
    }

    func setDuration(_ label: String, durationMs: UInt64) {
        self.durationMs = durationMs
        durationField.stringValue = label
    }

    func clearDuration() {
        durationMs = nil
        durationField.stringValue = ""
    }

    // MARK: - Internal

    private func updateElapsedLabel(seekLabel: String?) {
        if let seekLabel {
            elapsedField.stringValue = seekLabel
        }
        else if showRemainingTime {
            elapsedField.stringValue = currentRemaining
        }
        else {
            elapsedField.stringValue = currentElapsed
        }
    }

    @objc
    private func sliderValueChanged(_: SeekSlider) {
        if slider.isDragging, let durationMs {
            let label =
                showRemainingTime
                ? formatRemainingAtRatio(
                    ratio: slider.doubleValue,
                    durationMs: durationMs
                )
                : formatTimeAtRatio(
                    ratio: slider.doubleValue,
                    durationMs: durationMs
                )
            updateElapsedLabel(seekLabel: label)
        }
    }

    private func seekCompleted(_ value: Double) {
        onSeek?(value)
        updateElapsedLabel(seekLabel: nil)
    }

    @objc
    private func elapsedTapped() {
        showRemainingTime.toggle()
        updateElapsedLabel(seekLabel: nil)
    }
}

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
    let durationLabel: String?
    let durationMs: UInt64?
    let onSeek: (Double) -> Void

    func makeCoordinator() -> Coordinator {
        Coordinator()
    }

    func makeNSView(context: Context) -> PlaybackProgressNSView {
        let view = PlaybackProgressNSView()
        view.onSeek = onSeek
        applyDuration(to: view)
        context.coordinator.subscribe(to: positionPublisher, view: view)
        return view
    }

    func updateNSView(_ view: PlaybackProgressNSView, context _: Context) {
        view.onSeek = onSeek
        applyDuration(to: view)
    }

    private func applyDuration(to view: PlaybackProgressNSView) {
        if let durationLabel, let durationMs {
            view.setDuration(durationLabel, durationMs: durationMs)
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
            view: PlaybackProgressNSView
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
