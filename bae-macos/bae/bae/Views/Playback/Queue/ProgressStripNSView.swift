import AppKit
import BaeKit
import SwiftUI

/// A compact, display-only playback readout: a slim accent bar plus an elapsed
/// label, set straight from the playing track's position stream once per
/// display frame for the same reason as `SeekBarNSView` — core sends positions
/// about as often as the display draws frames, too often for SwiftUI
/// observation. Unlike the seek bar it takes no input: the queue panel's
/// now-playing card shows where playback is; seeking stays with the transport
/// bar, and a seek made there shows here too, since core sends the seek's
/// target as the track's position.
final class ProgressStripNSView: NSView {
    private let bar: ProgressTrackNSView
    private(set) lazy var following = FrameFollowing<BridgeTrackPosition>(
        view: self
    ) { [weak self] position in
        guard let self else { return }
        if let position {
            setPosition(position)
        }
        else {
            reset()
        }
    }

    var accent: NSColor {
        get { bar.accent }
        set { bar.accent = newValue }
    }
    private let elapsedField: NSTextField

    init(accent: NSColor) {
        bar = ProgressTrackNSView(progress: 0, accent: accent)
        bar.setAccessibilityLabel(String(localized: "Playback position"))
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

    // MARK: - Position updates

    /// Draw `position` from now on, or nothing.
    func follow(_ position: LatestSource<BridgeTrackPosition>?) {
        guard let position else {
            following.stop()
            reset()
            return
        }
        following.follow(position)
    }

    func stopFollowing() {
        following.stop()
    }

    /// Sets the bar and label to `position`, as core sent it.
    private func setPosition(_ position: BridgeTrackPosition) {
        if bar.progress != position.progress {
            bar.progress = position.progress
        }
        elapsedField.show(
            DurationClock.seekBar(
                positionMs: position.positionMs,
                durationMs: position.durationMs,
                showRemaining: false
            )
            .leading
        )
    }

    /// Sets the bar to 0 and empties the label.
    private func reset() {
        bar.progress = 0
        elapsedField.show("")
    }
}

// MARK: - SwiftUI bridge

struct ProgressStripRepresentable: NSViewRepresentable {
    @Environment(\.accentChoice)
    private var accent
    @Environment(\.colorScheme)
    private var colorScheme
    /// Where the playing track is; `nil` while no track plays.
    let position: LatestSource<BridgeTrackPosition>?

    func makeNSView(context _: Context) -> ProgressStripNSView {
        let view = ProgressStripNSView(
            accent: NSColor(accent.color(in: colorScheme))
        )
        view.follow(position)
        return view
    }

    func updateNSView(_ view: ProgressStripNSView, context _: Context) {
        view.accent = NSColor(accent.color(in: colorScheme))
        view.follow(position)
    }

    static func dismantleNSView(
        _ view: ProgressStripNSView,
        coordinator _: ()
    ) {
        view.stopFollowing()
    }
}
