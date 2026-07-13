import AppKit
import BaeKit

/// AppKit view for a seek slider flanked by elapsed/duration time labels,
/// updated imperatively (direct method calls from a Combine subscription)
/// rather than through SwiftUI observation — position ticks arrive at
/// display rate and would thrash the view tree (~15-20% CPU in NSHostingView
/// constraint recalculation when this went through SwiftUI).
///
/// Wrapped by `PlaybackProgressRepresentable` (now-playing bar) and
/// `PreviewProgressRepresentable` (import-tab preview player).
class SeekBarNSView: NSView {
    private let elapsedField: NSTextField
    private let slider: SeekSlider
    private let durationField: NSTextField
    /// Whether clicking the leading label switches it between elapsed and
    /// remaining time. Also widens that label (48pt vs 40pt) to fit the
    /// minus-prefixed countdown. The preview player has no such choice.
    private let showsRemainingTimeToggle: Bool

    var onSeek: ((Double) -> Void)?
    /// Called when the user clicks the leading label. The owner writes the new
    /// value to the config; the resulting invalidation sets `showRemainingTime`
    /// back on this view, which is what re-renders it.
    var onToggleRemainingTime: (() -> Void)?

    /// The user's choice, from the config. The view never stores it — a copy
    /// here is how it stopped following the user between devices.
    var showRemainingTime = false {
        didSet {
            if showRemainingTime != oldValue {
                updateLabels()
            }
        }
    }

    private var positionMs: UInt64 = 0
    private var durationMs: UInt64?
    /// Set while the user drags the slider: the dropped position, shown until
    /// the drag ends. Nil means the leading label follows playback.
    private var draggingPositionMs: UInt64?

    /// `fixedSliderWidth` pins the slider's width (the now-playing bar's
    /// 300pt track); nil lets the stack size it.
    init(showsRemainingTimeToggle: Bool, fixedSliderWidth: CGFloat?) {
        self.showsRemainingTimeToggle = showsRemainingTimeToggle

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
        stack.spacing = 8
        stack.translatesAutoresizingMaskIntoConstraints = false
        addSubview(stack)

        var constraints = [
            stack.leadingAnchor.constraint(equalTo: leadingAnchor),
            stack.trailingAnchor.constraint(equalTo: trailingAnchor),
            stack.centerYAnchor.constraint(equalTo: centerYAnchor),
            elapsedField.widthAnchor.constraint(
                equalToConstant: showsRemainingTimeToggle ? 48 : 40
            ),
            durationField.widthAnchor.constraint(equalToConstant: 40),
        ]
        if let fixedSliderWidth {
            constraints.append(
                slider.widthAnchor.constraint(equalToConstant: fixedSliderWidth)
            )
        }
        NSLayoutConstraint.activate(constraints)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError()
    }

    // MARK: - Direct position updates (called from a Combine subscription)

    func setPosition(progress: Double, positionMs: UInt64) {
        self.positionMs = positionMs

        if !slider.isDragging {
            slider.doubleValue = progress
            updateLabels()
        }
    }

    func setDuration(durationMs: UInt64) {
        self.durationMs = durationMs
        updateLabels()
    }

    func clearDuration() {
        durationMs = nil
        updateLabels()
    }

    /// Clears everything: slider to 0, both labels empty, duration dropped.
    func reset() {
        slider.doubleValue = 0
        positionMs = 0
        durationMs = nil
        draggingPositionMs = nil
        elapsedField.stringValue = ""
        durationField.stringValue = ""
    }

    // MARK: - Internal

    /// Both labels come from one core projection, so the leading one can never
    /// disagree with the trailing one about which clock it is showing. While the
    /// user drags, the leading label reads the dropped position instead of the
    /// playing one.
    private func updateLabels() {
        guard let durationMs else {
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
        guard slider.isDragging, let durationMs, durationMs > 0 else {
            return
        }
        draggingPositionMs = slider.positionMs(forDuration: durationMs)
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
