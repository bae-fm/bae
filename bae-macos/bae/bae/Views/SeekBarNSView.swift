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
    /// Clicking the elapsed label toggles it between elapsed and remaining
    /// time, persisted under the "showRemainingTime" UserDefaults key. Also
    /// widens the elapsed label (48pt vs 40pt) to fit the minus-prefixed
    /// remaining clock.
    private let showsRemainingTimeToggle: Bool

    var onSeek: ((Double) -> Void)?

    private var durationMs: UInt64?
    private var currentElapsed = ""
    private var currentRemaining = ""

    private var showRemainingTime: Bool {
        get { UserDefaults.standard.bool(forKey: "showRemainingTime") }
        set { UserDefaults.standard.set(newValue, forKey: "showRemainingTime") }
    }

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

    func setPosition(progress: Double, elapsed: String, remaining: String) {
        currentElapsed = elapsed
        currentRemaining = remaining

        if !slider.isDragging {
            slider.doubleValue = progress
            updateElapsedLabel(seekLabel: nil)
        }
    }

    /// For callers whose position events carry no remaining clock (the
    /// preview player). Only valid with the toggle disabled.
    func setPosition(progress: Double, elapsed: String) {
        assert(!showsRemainingTimeToggle)
        currentElapsed = elapsed

        if !slider.isDragging {
            slider.doubleValue = progress
            updateElapsedLabel(seekLabel: nil)
        }
    }

    func setDuration(durationMs: UInt64) {
        self.durationMs = durationMs
        durationField.stringValue = DurationClock.text(Int64(durationMs))
    }

    func clearDuration() {
        durationMs = nil
        durationField.stringValue = ""
    }

    /// Clears everything: slider to 0, both labels empty, duration dropped.
    func reset() {
        slider.doubleValue = 0
        elapsedField.stringValue = ""
        durationField.stringValue = ""
        durationMs = nil
        currentElapsed = ""
        currentRemaining = ""
    }

    // MARK: - Internal

    private func updateElapsedLabel(seekLabel: String?) {
        if let seekLabel {
            elapsedField.stringValue = seekLabel
        }
        else if showsRemainingTimeToggle && showRemainingTime {
            elapsedField.stringValue = currentRemaining
        }
        else {
            elapsedField.stringValue = currentElapsed
        }
    }

    @objc
    private func sliderValueChanged(_: SeekSlider) {
        guard slider.isDragging, let durationMs, durationMs > 0 else {
            return
        }
        let positionMs = slider.positionMs(forDuration: durationMs)
        let label =
            showsRemainingTimeToggle && showRemainingTime
            ? DurationClock.remaining(
                positionMs: positionMs,
                durationMs: durationMs
            )
            : DurationClock.text(Int64(positionMs))
        updateElapsedLabel(seekLabel: label)
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
