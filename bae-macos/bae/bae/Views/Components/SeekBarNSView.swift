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

/// One position as the bar draws it, from either stream.
private struct SeekPosition: Equatable {
    let progress: Double
    let positionMs: Int64
    /// `nil` when the track's length is unknown.
    let durationMs: UInt64?
    /// How many seeks playback has taken of the track, or `nil` for a stream
    /// that doesn't count them.
    let seeks: UInt64?
}

/// What a drop named, which the bar keeps drawing until playback takes the
/// seek: the position, the fill and how many seeks playback had taken then.
private struct DroppedSeek {
    let positionMs: Int64
    let progress: Double
    let seeks: UInt64
}

/// A seek slider between its time labels, set straight from its position
/// stream once per display frame rather than through SwiftUI observation,
/// since core sends positions about as often as the display draws frames.
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
        view: self
    ) { [weak self] position in
        self?
            .show(
                position.map {
                    SeekPosition(
                        progress: $0.progress,
                        positionMs: $0.positionMs,
                        durationMs: $0.durationMs,
                        seeks: $0.seeks
                    )
                }
            )
    }
    private(set) lazy var buffered = FrameFollowing<BridgeTrackBuffered>(
        view: self
    ) { [weak self] held in
        self?.slider.held = held?.ranges ?? []
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
                        durationMs: $0.durationMs,
                        seeks: nil
                    )
                }
            )
    }
    /// What the stream last said; `nil` while it holds nothing.
    private var position: SeekPosition?
    /// Set while the user drags the slider: the dragged position, shown until
    /// the drag ends. Nil means the leading label follows playback.
    private var draggingPositionMs: Int64?
    /// Set from the drop until playback takes the seek, which the stream says
    /// by counting it: the time and the fill stay at the dropped position
    /// while the stream still sends the playing one. Screen state only.
    private var dropped: DroppedSeek?
    /// The track source the bar follows, so a drop never outlives its track.
    private var followedTrack: LatestSource<BridgeTrackPosition>?

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
        slider.isContinuous = true
        slider.setAccessibilityLabel(String(localized: "Position"))
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
            if source != followedTrack { dropped = nil }
            followedTrack = source
            previews.stop()
            tracks.follow(source)
            buffered.follow(held)
        case .preview(let source):
            dropped = nil
            followedTrack = nil
            tracks.stop()
            buffered.stop()
            slider.held = []
            previews.follow(source)
        case nil:
            followedTrack = nil
            tracks.stop()
            buffered.stop()
            previews.stop()
            slider.held = []
            show(nil)
        }
    }

    func stopFollowing() {
        tracks.stop()
        buffered.stop()
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
            dropped = nil
            updateLabels()
            return
        }
        if let held = dropped, let seeks = shown.seeks, seeks > held.seeks {
            dropped = nil
        }
        slider.isEnabled = shown.durationMs != nil
        if !slider.isDragging && dropped == nil {
            slider.doubleValue = shown.progress
        }
        updateLabels()
    }

    // MARK: - Internal

    private static let idleLabelAlpha: CGFloat = 0.5

    /// Both labels come from one core projection, so they agree on which clock
    /// they show; during a drag the leading one reads the dropped position.
    private func updateLabels() {
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
            positionMs: draggingPositionMs ?? dropped?.positionMs
                ?? shown.positionMs,
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

    /// The drop keeps the dragged position and fill on the bar until the
    /// stream counts a seek past the one it showed at the drop: core names
    /// the seek's target as the position when it takes the seek, and every
    /// view drawing the track shows it from the next frame.
    private func seekCompleted(_ value: Double) {
        if let shown = position, let seeks = shown.seeks,
            let durationMs = shown.durationMs
        {
            dropped = DroppedSeek(
                positionMs: draggingPositionMs
                    ?? Int64(slider.positionMs(forDuration: durationMs)),
                progress: value,
                seeks: seeks
            )
        }
        draggingPositionMs = nil
        onSeek?(value)
        updateLabels()
    }

    @objc
    private func elapsedTapped() {
        onToggleRemainingTime?()
    }
}
