import BaeKit
import SwiftUI

/// The sheep over the word "bae". The word is in the primary text colour.
/// With `glow` a soft pink and purple glow sits behind the sheep and a pool
/// of light under it, both still, and the word takes the sheep's cream and a
/// thin edge to its lower right; with `bob` the
/// sheep and the word each float and tilt (unless motion is reduced). The logo
/// guideline (`the-sheep-logo-and-its-forms.md`, which
/// `design/system/README.md` lists) is the source of every proportion and
/// timing here; the colours are `Theme`'s `logoGlow…`, `logoPool…` and `logoWord…`.
struct Logo: View {
    /// The sheep's width; the word and the glow scale with it.
    let size: CGFloat
    let bob: Bool
    let glow: Bool

    @Environment(\.accessibilityReduceMotion)
    private var reduceMotion
    @Environment(\.clockNow)
    private var fixedNow

    private enum Proportion {
        static let sheepHeight = 545.0 / 584.0
        static let wordWidth = 0.52
        static let wordAspect = 1556.0 / 736.0
        static let gap = 0.06
        static let glowWidth = 2.6
        static let glowCentre = 0.65
        static let poolWidth = 1.25
        static let poolHeight = 0.34
        static let poolCentre = 0.90
        /// Where the pool's gradient ends, as a fraction of its width: the
        /// ellipse's far corner, as the design's CSS `radial-gradient(ellipse)`
        /// measures it.
        static let poolReach = 0.5 * 2.0.squareRoot()
        static let sheepRise = 0.04
        static let wordRise = 0.15
    }

    private enum Motion {
        static let risePeriod = 6.0
        static let tiltPeriod = 7.3
        static let tiltDegrees = 2.0
        static let wordDelay = 2.0
    }

    private var sheepHeight: CGFloat { size * Proportion.sheepHeight }
    // The word's box and the gap are whole points, as the board and the
    // Android app draw them.
    private var wordWidth: CGFloat { (size * Proportion.wordWidth).rounded() }
    private var wordHeight: CGFloat {
        (wordWidth / Proportion.wordAspect).rounded()
    }
    private var gap: CGFloat { (size * Proportion.gap).rounded() }

    var body: some View {
        mark
    }

    private var mark: some View {
        let still = reduceMotion || !bob || fixedNow != nil
        return TimelineView(.animation(paused: still)) { timeline in
            let t = timeline.date.timeIntervalSinceReferenceDate
            VStack(spacing: gap) {
                // The sheep's slot is the glow's anchor, so the glow stays
                // still while the sheep floats over it.
                Color.clear
                    .frame(width: size, height: sheepHeight)
                    .background {
                        if glow {
                            glowLayers
                        }
                    }
                    .overlay {
                        // The word names the logo; the sheep is decoration.
                        Image("Sheep")
                            .resizable()
                            .scaledToFit()
                            .accessibilityHidden(true)
                            .modifier(
                                Bob(
                                    reduced: still,
                                    time: t,
                                    rise: Proportion.sheepRise * sheepHeight
                                )
                            )
                    }
                // The word's box is the logo's one element, as on the
                // board, so the drawings are behind it: the word fits inside
                // the box, a fraction of a point narrower, and its edge
                // reaches past it.
                Color.clear
                    .frame(width: wordWidth, height: wordHeight)
                    .accessibilityElement()
                    .accessibilityLabel(Text(verbatim: "bae"))
                    .accessibilityAddTraits(.isImage)
                    .background {
                        ZStack {
                            // The word's edge, a copy 1 point right and
                            // below it. A layer shadow of no blur is drawn
                            // above the word in a macOS window
                            // (`ShadowedFace`).
                            if glow {
                                wordImage(Theme.logoWordEdge)
                                    .offset(x: 1, y: 1)
                            }
                            wordImage(glow ? Theme.logoWord : Theme.textPrimary)
                        }
                        .frame(width: wordWidth, height: wordHeight)
                        .accessibilityHidden(true)
                    }
                    .modifier(
                        Bob(
                            reduced: still,
                            time: t - Motion.wordDelay,
                            rise: Proportion.wordRise * wordHeight
                        )
                    )
            }
        }
    }

    private func wordImage(_ color: some ShapeStyle) -> some View {
        Image("LogoWord")
            .resizable()
            .scaledToFit()
            .foregroundStyle(color)
    }

    /// The disc of light centred 65% down the sheep and the pool under it,
    /// which take no layout.
    private var glowLayers: some View {
        ZStack {
            Circle()
                .fill(
                    RadialGradient(
                        stops: [
                            .init(color: Theme.logoGlowCentre, location: 0),
                            .init(color: Theme.logoGlowInner, location: 0.30),
                            .init(color: Theme.logoGlowMiddle, location: 0.55),
                            .init(color: Theme.logoGlowOuter, location: 0.80),
                            .init(color: .clear, location: 1),
                        ],
                        center: .center,
                        startRadius: 0,
                        endRadius: size * Proportion.glowWidth / 2
                    )
                )
                .frame(
                    width: size * Proportion.glowWidth,
                    height: size * Proportion.glowWidth
                )
                .offset(y: sheepHeight * (Proportion.glowCentre - 0.5))
            Ellipse()
                .fill(
                    EllipticalGradient(
                        stops: [
                            .init(color: Theme.logoPoolCentre, location: 0),
                            .init(color: Theme.logoPoolMiddle, location: 0.5),
                            .init(color: .clear, location: 0.72),
                        ],
                        endRadiusFraction: Proportion.poolReach
                    )
                )
                .frame(
                    width: size * Proportion.poolWidth,
                    height: sheepHeight * Proportion.poolHeight
                )
                .offset(y: sheepHeight * (Proportion.poolCentre - 0.5))
        }
    }

    /// Rises by `rise` and back every 6 seconds and tilts 2° each way every
    /// 7.3, each from rest at `time` 0; none when motion is reduced.
    private struct Bob: ViewModifier {
        let reduced: Bool
        let time: Double
        let rise: CGFloat

        func body(content: Content) -> some View {
            content
                .offset(
                    y: reduced
                        ? 0 : -rise * (1 - cos(phase(Motion.risePeriod))) / 2
                )
                .rotationEffect(
                    .degrees(
                        reduced
                            ? 0
                            : -Motion.tiltDegrees
                                * cos(phase(Motion.tiltPeriod))
                    )
                )
        }

        private func phase(_ period: Double) -> Double {
            2 * .pi * time / period
        }
    }
}
