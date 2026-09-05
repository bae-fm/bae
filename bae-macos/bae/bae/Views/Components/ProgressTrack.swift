import AppKit
import BaeKit
import SwiftUI

/// The one way a progress bar is drawn in this app: a rounded low-opacity
/// track with an accent fill pill. `ProgressTrackNSView` renders it,
/// `SlimSeekSliderCell` calls the same drawing under the seek knob math, and
/// `ProgressTrackBar` exposes it to SwiftUI layouts as a value prop — the
/// drawing and the indeterminate animation stay in AppKit/Core Animation
/// either way, so no progress rendering rides the SwiftUI render loop.
enum ProgressTrackDrawing {
    /// Draws the track pill and, for a positive fraction, the accent fill pill
    /// (never narrower than its own height, so a tiny fraction still reads as
    /// a pill rather than a sliver).
    static func draw(in bar: NSRect, fraction: Double?, accent: NSColor) {
        let radius = bar.height / 2
        NSColor.labelColor.withAlphaComponent(0.12).setFill()
        NSBezierPath(roundedRect: bar, xRadius: radius, yRadius: radius).fill()

        guard let fraction else {
            return
        }
        let clamped = CGFloat(min(max(fraction, 0), 1))
        guard clamped > 0 else {
            return
        }
        var fill = bar
        fill.size.width = max(bar.width * clamped, bar.height)
        let fillPath = NSBezierPath(
            roundedRect: fill,
            xRadius: radius,
            yRadius: radius
        )
        accent.setFill()
        fillPath.fill()
    }
}

/// The shared progress bar. `progress` in 0...1 draws a determinate fill;
/// `nil` runs an indeterminate marching pill, animated by Core Animation so
/// nothing ticks the view hierarchy.
final class ProgressTrackNSView: NSView {
    /// Height of the drawn track, the same on every surface that draws one.
    /// Also the view's intrinsic height.
    static let trackHeight: CGFloat = 4

    var progress: Double? {
        didSet {
            if progress != oldValue {
                updateIndeterminateLayer()
                needsDisplay = true
            }
        }
    }

    private let indeterminateClip = CALayer()
    private let indeterminatePill = CALayer()
    private static let marchAnimationKey = "march"

    var accent: NSColor {
        didSet {
            indeterminatePill.backgroundColor = accent.cgColor
            needsDisplay = true
        }
    }

    init(accent: NSColor) {
        self.accent = accent
        super.init(frame: .zero)
        wantsLayer = true

        indeterminatePill.backgroundColor = accent.cgColor
        indeterminateClip.masksToBounds = true
        indeterminateClip.addSublayer(indeterminatePill)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError()
    }

    override var intrinsicContentSize: NSSize {
        NSSize(width: NSView.noIntrinsicMetric, height: Self.trackHeight)
    }

    private var trackRect: NSRect {
        NSRect(
            x: 0,
            y: (bounds.height - Self.trackHeight) / 2,
            width: bounds.width,
            height: Self.trackHeight
        )
    }

    override func draw(_: NSRect) {
        ProgressTrackDrawing.draw(
            in: trackRect,
            fraction: progress,
            accent: accent
        )
    }

    override func layout() {
        super.layout()
        layoutIndeterminateLayer()
    }

    // MARK: - Indeterminate marching pill

    private func updateIndeterminateLayer() {
        if progress == nil {
            if indeterminateClip.superlayer == nil {
                layer?.addSublayer(indeterminateClip)
            }
            layoutIndeterminateLayer()
        }
        else {
            indeterminatePill.removeAnimation(
                forKey: Self.marchAnimationKey
            )
            indeterminateClip.removeFromSuperlayer()
        }
    }

    private func layoutIndeterminateLayer() {
        guard progress == nil, bounds.width > 0 else {
            return
        }
        let track = trackRect
        indeterminateClip.frame = track
        indeterminateClip.cornerRadius = track.height / 2
        let pillWidth = max(track.width * 0.35, track.height)
        indeterminatePill.frame = CGRect(
            x: 0,
            y: 0,
            width: pillWidth,
            height: track.height
        )
        indeterminatePill.cornerRadius = track.height / 2

        indeterminatePill.removeAnimation(forKey: Self.marchAnimationKey)
        let march = CABasicAnimation(keyPath: "position.x")
        march.fromValue = -pillWidth / 2
        march.toValue = track.width + pillWidth / 2
        march.duration = 1.4
        march.repeatCount = .infinity
        indeterminatePill.add(march, forKey: Self.marchAnimationKey)
    }
}

/// SwiftUI wrapper for the value-driven sites: the value arrives as a prop,
/// the rendering stays in AppKit. A bar with text describing it is drawn by
/// `ProgressLine`; this alone is for a bar that stands by itself.
struct ProgressTrackBar: NSViewRepresentable {
    @Environment(\.accentChoice)
    private var accent
    @Environment(\.colorScheme)
    private var colorScheme
    /// 0...1 for a determinate fill; nil for the indeterminate marching pill.
    var progress: Double?

    func makeNSView(context _: Context) -> ProgressTrackNSView {
        let view = ProgressTrackNSView(
            accent: NSColor(accent.color(in: colorScheme))
        )
        view.progress = progress
        return view
    }

    func updateNSView(_ view: ProgressTrackNSView, context _: Context) {
        view.accent = NSColor(accent.color(in: colorScheme))
        view.progress = progress
    }

    func sizeThatFits(
        _ proposal: ProposedViewSize,
        nsView _: ProgressTrackNSView,
        context _: Context
    ) -> CGSize? {
        CGSize(
            width: proposal.width ?? 0,
            height: ProgressTrackNSView.trackHeight
        )
    }
}

#if DEBUG
    #Preview("Progress Track") {
        VStack(alignment: .leading, spacing: 16) {
            ProgressTrackBar(progress: 0)
            ProgressTrackBar(progress: 0.01)
            ProgressTrackBar(progress: 0.4)
            ProgressTrackBar(progress: 1)
            ProgressTrackBar(progress: nil)
            ProgressTrackBar(progress: 0.4)
                .frame(width: 140)
        }
        .padding()
        .frame(width: 360)
        .windowBackground()
    }
#endif
