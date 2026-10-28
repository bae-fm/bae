import AppKit
import BaeKit

/// The stream a seek bar draws: the playing track's position, or the
/// preview's.
enum SeekBarSource {
    case track(LatestSource<BridgeTrackPosition>)
    case preview(LatestSource<BridgePreviewPosition>)
}

/// One position as the bar draws it, from either stream.
private struct SeekPosition: Equatable {
    let progress: Double
    let positionMs: Int64
    /// `nil` when the track's length is unknown.
    let durationMs: UInt64?
}

/// A seek slider between its time labels, set straight from its position
/// stream once per display frame rather than through SwiftUI observation,
/// since a position arrives with every tick core sends.
final class SeekBarNSView: NSView {
    private let elapsedField: NSTextField
    private let slider: SeekSlider
    private let durationField: NSTextField
    /// Whether clicking the leading label switches it between elapsed and
    /// remaining time, which widens it to fit the minus sign.
    private let showsRemainingTimeToggle: Bool

    var accent: NSColor {
        get { slider.accent }
        set { slider.accent = newValue }
    }

    var onSeek: ((Double) -> Void)?
    /// Called on a click on the leading label; the owner writes the config,
    /// which sets `showRemainingTime` back here.
    var onToggleRemainingTime: (() -> Void)?

    /// Follows the config; clicks go through `onToggleRemainingTime`.
    var showRemainingTime = false {
        didSet {
            if showRemainingTime != oldValue {
                updateLabels()
            }
        }
    }

    private(set) lazy var tracks = FrameFollowing<BridgeTrackPosition>(
        view: self
    ) {
        [weak self] position in
        self?
            .show(
                position.map {
                    SeekPosition(
                        progress: $0.progress,
                        positionMs: $0.positionMs,
                        durationMs: $0.durationMs
                    )
                }
            )
    }
    private(set) lazy var previews = FrameFollowing<BridgePreviewPosition>(
        view: self
    ) { [weak self] position in
        self?
            .show(
                position.map {
                    SeekPosition(
                        progress: $0.progress,
                        positionMs: Int64($0.positionMs),
                        durationMs: $0.durationMs
                    )
                }
            )
    }
    /// What the stream last said; `nil` while it holds nothing.
    private var position: SeekPosition?
    /// The playing track's position, which a seek the person makes is held
    /// in front of until core applies it; `nil` while the bar draws a
    /// preview or nothing, whose seeks are not held.
    private var track: LatestSource<BridgeTrackPosition>?
    /// Set while the user drags the slider: the dropped position, shown until
    /// the drag ends. Nil means the leading label follows playback.
    private var draggingPositionMs: Int64?

    init(showsRemainingTimeToggle: Bool) {
        self.showsRemainingTimeToggle = showsRemainingTimeToggle

        let font = NSFont.monospacedDigitSystemFont(
            ofSize: ThemeText.fine.macOSSize,
            weight: ThemeText.fine.nsWeight
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

    // MARK: - Position updates

    /// Draw `source` from now on, or nothing.
    func follow(_ source: SeekBarSource?) {
        switch source {
        case .track(let source):
            previews.stop()
            track = source
            tracks.follow(source)
        case .preview(let source):
            tracks.stop()
            track = nil
            previews.follow(source)
        case nil:
            tracks.stop()
            previews.stop()
            track = nil
            show(nil)
        }
    }

    func stopFollowing() {
        tracks.stop()
        previews.stop()
    }

    /// Position, duration, and progress always belong to the same value. A
    /// track whose length is unknown has no fill to seek to, so the slider is
    /// disabled until one is known.
    private func show(_ position: SeekPosition?) {
        self.position = position
        guard let shown = position else {
            slider.isEnabled = false
            slider.doubleValue = 0
            draggingPositionMs = nil
            updateLabels()
            return
        }
        slider.isEnabled = shown.durationMs != nil
        if !slider.isDragging {
            slider.doubleValue = shown.progress
        }
        updateLabels()
    }

    // MARK: - Internal

    /// Both labels come from one core projection, so they agree on which clock
    /// they show; during a drag the leading one reads the dropped position.
    private func updateLabels() {
        guard let shown = position else {
            elapsedField.show("")
            durationField.show("")
            return
        }
        let labels = DurationClock.seekBar(
            positionMs: draggingPositionMs ?? shown.positionMs,
            durationMs: shown.durationMs,
            showRemaining: showsRemainingTimeToggle && showRemainingTime
        )
        elapsedField.show(labels.leading)
        durationField.show(labels.trailing)
    }

    @objc
    private func sliderValueChanged(_: SeekSlider) {
        guard slider.isDragging,
            let durationMs = position?.durationMs
        else {
            return
        }
        draggingPositionMs = Int64(slider.positionMs(forDuration: durationMs))
        updateLabels()
    }

    /// A seek on the playing track is held in its position stream, so every
    /// view drawing the track shows where the person dropped it until core
    /// applies it; this bar draws it at once.
    private func seekCompleted(_ value: Double) {
        draggingPositionMs = nil
        if let track {
            track.holdSeek(at: value)
            tracks.frame()
        }
        onSeek?(value)
        updateLabels()
    }

    @objc
    private func elapsedTapped() {
        onToggleRemainingTime?()
    }
}
