import AppKit
import BaeKit
import SwiftUI

/// A compact, display-only playback readout: a slim accent bar plus an elapsed
/// label, set straight from the playing track's position stream once per
/// display frame for the same reason as `SeekBarNSView` — the position moves
/// every frame, too often for SwiftUI observation. Unlike the
/// seek bar it takes no input: the queue panel's now-playing card shows where
/// playback is; seeking stays with the transport bar, and a seek made there
/// shows here too, held in the track's stream until core applies it.
final class ProgressStripNSView: NSView {
    private let bar: ProgressTrackNSView
    private(set) lazy var following = FrameFollowing<BridgeTrackPosition>(
        view: self,
        show: { [weak self] position in
            guard let self else { return }
            anchor = position?.anchor
            if anchor != nil {
                draw(atNs: FrameClock.nowNs())
            }
            else {
                reset()
            }
        },
        onFrame: { [weak self] _, frameNs in
            self?.draw(atNs: frameNs)
        }
    )

    var accent: NSColor {
        get { bar.accent }
        set { bar.accent = newValue }
    }
    private let elapsedField: NSTextField

    /// Nil until a position arrives and after a reset; the label is empty then.
    private var anchor: PlaybackAnchor?

    init(accent: NSColor) {
        bar = ProgressTrackNSView(progress: 0, accent: accent)
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

    /// The bar and label at a frame timed `frameNs`, from the anchor.
    private func draw(atNs frameNs: UInt64) {
        guard let anchor else { return }
        let progress = anchor.progress(atFrameNs: frameNs)
        if bar.progress != progress {
            bar.progress = progress
        }
        elapsedField.show(
            DurationClock.seekBar(
                positionMs: anchor.positionMs(atFrameNs: frameNs),
                durationMs: anchor.durationMs,
                showRemaining: false
            )
            .leading
        )
    }

    /// Clears everything: bar to 0, label empty, anchor dropped.
    private func reset() {
        bar.progress = 0
        anchor = nil
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
