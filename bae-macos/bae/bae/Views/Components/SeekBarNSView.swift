import AppKit
import BaeKit

/// AppKit view for a seek slider flanked by elapsed/duration time labels,
/// updated imperatively (direct method calls from a Combine subscription)
/// rather than through SwiftUI observation — position ticks arrive at
/// display rate and would thrash the view tree (~15-20% CPU in NSHostingView
/// constraint recalculation when this went through SwiftUI).
///
/// `SeekBarRepresentable` supplies complete timeline updates for library
/// playback and import audition.
final class SeekBarNSView: NSView {
    private let elapsedField: NSTextField
    private let slider: SeekSlider
    private let durationField: NSTextField
    /// Whether clicking the leading label switches it between elapsed and
    /// remaining time. Also widens that label (48pt vs 40pt) to fit the
    /// minus-prefixed countdown. The preview player has no such choice.
    private let showsRemainingTimeToggle: Bool

    var accent: NSColor {
        get { slider.accent }
        set { slider.accent = newValue }
    }

    var onSeek: ((Double) -> Void)?
    /// Called when the user clicks the leading label. The owner writes the new
    /// value to the config; its subscription sets `showRemainingTime`
    /// back on this view, which is what re-renders it.
    var onToggleRemainingTime: (() -> Void)?

    /// The rendered preference follows config updates; clicks write through
    /// `onToggleRemainingTime` rather than changing this value locally.
    var showRemainingTime = false {
        didSet {
            if showRemainingTime != oldValue {
                updateLabels()
            }
        }
    }

    private var position: PlaybackPositionEvent = .reset
    /// Set while the user drags the slider: the dropped position, shown until
    /// the drag ends. Nil means the leading label follows playback.
    private var draggingPositionMs: Int64?

    init(showsRemainingTimeToggle: Bool) {
        self.showsRemainingTimeToggle = showsRemainingTimeToggle

        let font = NSFont.monospacedDigitSystemFont(
            ofSize: 11.5,
            weight: .semibold
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

        super.init(frame: .zero)

        slider.target = self
        slider.action = #selector(sliderValueChanged(_:))
        slider.onSeekComplete = { [weak self] value in
            self?.seekCompleted(value)
        }

        if showsRemainingTimeToggle {
            let click = NSClickGestureRecognizer(
                target: self,
                action: #selector(elapsedTapped)
            )
            elapsedField.addGestureRecognizer(click)
        }

        let stack = NSStackView(views: [elapsedField, slider, durationField])
        stack.orientation = .horizontal
        stack.spacing = 11
        stack.translatesAutoresizingMaskIntoConstraints = false
        addSubview(stack)

        NSLayoutConstraint.activate([
            stack.leadingAnchor.constraint(equalTo: leadingAnchor),
            stack.trailingAnchor.constraint(equalTo: trailingAnchor),
            stack.centerYAnchor.constraint(equalTo: centerYAnchor),
            elapsedField.widthAnchor.constraint(
                equalToConstant: showsRemainingTimeToggle ? 48 : 40
            ),
            durationField.widthAnchor.constraint(equalToConstant: 40),
        ])
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError()
    }

    // MARK: - Direct position updates (called from a Combine subscription)

    /// Position, duration, and progress always belong to the same update.
    func apply(_ position: PlaybackPositionEvent) {
        self.position = position
        switch position {
        case .position(let progress, _, _):
            guard !slider.isDragging else { return }
            slider.doubleValue = progress
        case .reset:
            slider.doubleValue = 0
            draggingPositionMs = nil
        }
        updateLabels()
    }

    // MARK: - Internal

    /// Both labels come from one core projection, so the leading one can never
    /// disagree with the trailing one about which clock it is showing. While the
    /// user drags, the leading label reads the dropped position instead of the
    /// playing one.
    private func updateLabels() {
        guard case .position(_, let positionMs, let durationMs) = position
        else {
            elapsedField.stringValue = ""
            durationField.stringValue = ""
            return
        }
        let labels = DurationClock.seekBar(
            positionMs: draggingPositionMs ?? positionMs,
            durationMs: durationMs,
            showRemaining: showsRemainingTimeToggle && showRemainingTime
        )
        elapsedField.stringValue = labels.leading
        durationField.stringValue = labels.trailing
    }

    @objc
    private func sliderValueChanged(_: SeekSlider) {
        guard slider.isDragging,
            case .position(_, _, let durationMs) = position,
            durationMs > 0
        else {
            return
        }
        draggingPositionMs = Int64(slider.positionMs(forDuration: durationMs))
        updateLabels()
    }

    private func seekCompleted(_ value: Double) {
        draggingPositionMs = nil
        onSeek?(value)
        updateLabels()
    }

    @objc
    private func elapsedTapped() {
        onToggleRemainingTime?()
    }
}
