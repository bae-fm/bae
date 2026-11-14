import AppKit
import BaeKit

/// The stream a seek bar draws: the playing track's position over the
/// parts of it whose bytes are here, or the shot's position.
enum SeekBarSource {
    case track(
        LatestSource<BridgeTrackPosition>,
        buffered: LatestSource<BridgeTrackBuffered>
    )
    case preview(LatestSource<BridgePreviewPosition>)
}

/// A seek slider between its time labels, set straight from its position
/// stream once per display frame rather than through SwiftUI observation:
/// the stream holds an anchor, and each frame works out the position from it
/// and the frame's time.
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
    /// Whether the labels read "0:00" at half strength while nothing plays,
    /// instead of showing nothing.
    var showsIdleClock = false {
        didSet {
            if showsIdleClock != oldValue {
                updateLabels()
            }
        }
    }

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
        view: self,
        show: { [weak self] position in
            self?.show(position?.anchor)
        },
        onFrame: { [weak self] _, frameNs in
            self?.draw(atNs: frameNs)
        }
    )
    private(set) lazy var buffered = FrameFollowing<BridgeTrackBuffered>(
        view: self
    ) { [weak self] held in
        self?.slider.held = held?.ranges ?? []
    }
    private(set) lazy var previews = FrameFollowing<BridgePreviewPosition>(
        view: self
    ) { [weak self] position in
        self?.show(position?.anchor)
    }
    /// What the stream last said; `nil` while it holds nothing.
    private var position: PlaybackAnchor?
    /// Holds a seek the person makes in front of the stream's position
    /// until core applies it, and draws the stream again at once; `nil`
    /// while the bar draws a shot or nothing, whose seeks are not held.
    private var holdSeek: ((Double) -> Void)?
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
        case .track(let source, let held):
            previews.stop()
            holdSeek = { [weak self] progress in
                source.holdSeek(at: progress)
                self?.tracks.frame()
            }
            tracks.follow(source)
            buffered.follow(held)
        case .preview(let source):
            tracks.stop()
            buffered.stop()
            slider.held = []
            holdSeek = { [weak self] progress in
                source.holdSeek(at: progress)
                self?.previews.frame()
            }
            previews.follow(source)
        case nil:
            tracks.stop()
            buffered.stop()
            previews.stop()
            slider.held = []
            holdSeek = nil
            show(nil)
        }
    }

    func stopFollowing() {
        tracks.stop()
        buffered.stop()
        previews.stop()
    }

    /// Position, duration, and progress always belong to the same anchor. A
    /// track whose length is unknown has no fill to seek to, so the slider is
    /// disabled until one is known.
    private func show(_ anchor: PlaybackAnchor?) {
        position = anchor
        guard anchor != nil else {
            slider.isEnabled = false
            slider.doubleValue = 0
            draggingPositionMs = nil
            updateLabels(atNs: FrameClock.nowNs())
            return
        }
        draw(atNs: FrameClock.nowNs())
    }

    /// The slider and labels at a frame timed `frameNs`, from the anchor.
    private func draw(atNs frameNs: UInt64) {
        guard let shown = position else { return }
        slider.isEnabled = shown.durationMs != nil
        let progress = shown.progress(atFrameNs: frameNs)
        if !slider.isDragging, slider.doubleValue != progress {
            slider.doubleValue = progress
        }
        updateLabels(atNs: frameNs)
    }

    // MARK: - Internal

    private static let idleLabelAlpha: CGFloat = 0.5

    /// Both labels come from one core projection, so they agree on which clock
    /// they show; during a drag the leading one reads the dropped position.
    private func updateLabels(atNs frameNs: UInt64 = FrameClock.nowNs()) {
        guard let shown = position else {
            let idle =
                showsIdleClock
                ? DurationClock.seekBar(
                    positionMs: 0,
                    durationMs: 0,
                    showRemaining: false
                ) : (leading: "", trailing: "")
            elapsedField.show(idle.leading)
            durationField.show(idle.trailing)
            elapsedField.alphaValue = Self.idleLabelAlpha
            durationField.alphaValue = Self.idleLabelAlpha
            return
        }
        elapsedField.alphaValue = 1
        durationField.alphaValue = 1
        let labels = DurationClock.seekBar(
            positionMs: draggingPositionMs
                ?? shown.positionMs(atFrameNs: frameNs),
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

    /// A seek on the playing track or the preview is held in its position
    /// stream, so every view drawing it shows where the person dropped it
    /// until core applies it; this bar draws it at once.
    private func seekCompleted(_ value: Double) {
        draggingPositionMs = nil
        holdSeek?(value)
        onSeek?(value)
        updateLabels()
    }

    @objc
    private func elapsedTapped() {
        onToggleRemainingTime?()
    }
}
