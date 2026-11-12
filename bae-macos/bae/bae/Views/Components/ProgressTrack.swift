import AppKit
import BaeKit
import SwiftUI

/// The app's one progress bar drawing: a rounded hairline track with an accent
/// fill, kept in AppKit so no progress rendering runs through SwiftUI.
enum ProgressTrackDrawing {
    /// Draws the track and, for a positive fraction, an accent fill never
    /// narrower than its height.
    static func draw(in bar: NSRect, fraction: Double?, accent: NSColor) {
        drawTrack(in: bar)
        guard let fraction else {
            return
        }
        drawFill(in: bar, fraction: fraction, accent: accent)
    }

    /// The empty track.
    static func drawTrack(in bar: NSRect) {
        let radius = bar.height / 2
        NSColor(Theme.hairline).setFill()
        NSBezierPath(roundedRect: bar, xRadius: radius, yRadius: radius).fill()
    }

    /// Each of `spans`, fractions of the track from its start, in `color`
    /// over the track.
    static func drawSpans(
        _ spans: [BridgeTrackSpan],
        in bar: NSRect,
        color: NSColor
    ) {
        let radius = bar.height / 2
        color.setFill()
        for span in spans {
            var stretch = bar
            stretch.origin.x = bar.minX + bar.width * CGFloat(span.start)
            stretch.size.width = bar.width * CGFloat(span.end - span.start)
            NSBezierPath(roundedRect: stretch, xRadius: radius, yRadius: radius)
                .fill()
        }
    }

    /// An accent fill to `fraction` of the track, never narrower than its
    /// height; none at zero.
    static func drawFill(in bar: NSRect, fraction: Double, accent: NSColor) {
        let radius = bar.height / 2
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

/// The shared progress bar: a fill for 0...1, or for `nil` a marching pill
/// animated by Core Animation so nothing ticks the view hierarchy.
class ProgressTrackNSView: NSView {
    /// Height of the drawn track and of the view, in a pane's or row's line.
    static let trackHeight: CGFloat = 4
    /// The design system's track height, for a line that stands alone on a
    /// screen: a loading or restore screen's step over its bar.
    static let stageHeight: CGFloat = 6

    let height: CGFloat

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

    init(
        progress: Double?,
        accent: NSColor,
        height: CGFloat = ProgressTrackNSView.trackHeight
    ) {
        self.progress = progress
        self.accent = accent
        self.height = height
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

    override func isAccessibilityElement() -> Bool { true }

    override func accessibilityRole() -> NSAccessibility.Role? {
        .progressIndicator
    }

    /// The percent done; none while the length is unknown.
    override func accessibilityValue() -> Any? {
        progress.map { Int(($0 * 100).rounded()) }
    }

    override var intrinsicContentSize: NSSize {
        NSSize(width: NSView.noIntrinsicMetric, height: height)
    }

    private var trackRect: NSRect {
        NSRect(
            x: 0,
            y: (bounds.height - height) / 2,
            width: bounds.width,
            height: height
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
        updateIndeterminateLayer()
    }

    // MARK: - Indeterminate marching pill

    /// Puts the pill in the layer tree exactly while `progress` is nil,
    /// including a view made with nil, whose `didSet` never runs.
    private func updateIndeterminateLayer() {
        guard progress == nil else {
            indeterminatePill.removeAnimation(
                forKey: Self.marchAnimationKey
            )
            indeterminateClip.removeFromSuperlayer()
            return
        }
        if indeterminateClip.superlayer == nil {
            layer?.addSublayer(indeterminateClip)
        }
        guard bounds.width > 0 else {
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

/// SwiftUI wrapper for a bar on its own; `MacProgressLine` draws one with text.
struct ProgressTrackBar: NSViewRepresentable {
    @Environment(\.accentChoice)
    private var accent
    @Environment(\.colorScheme)
    private var colorScheme
    /// 0...1 for a determinate fill; nil for the indeterminate marching pill.
    var progress: Double?
    /// The fill's colour, where the bar stands for one kind of transfer;
    /// `nil` fills with the window's accent.
    var tint: Color?

    func makeNSView(context _: Context) -> ProgressTrackNSView {
        ProgressTrackNSView(progress: progress, accent: fill)
    }

    func updateNSView(_ view: ProgressTrackNSView, context _: Context) {
        view.accent = fill
        view.progress = progress
    }

    private var fill: NSColor {
        NSColor(tint ?? accent.color(in: colorScheme))
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
        VStack(alignment: .leading, spacing: ThemeSpace.group) {
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
