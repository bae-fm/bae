import BaeKit
import SwiftUI

/// The sheep over the word "bae", as Welcome draws them: a soft pink and
/// purple glow behind the sheep and a pool of light under it, both still,
/// while the sheep and the word each float and tilt. The README's Logo section
/// (`design/system/README.md`) is the source of every proportion and timing
/// here; the colours are `Theme`'s `logoGlow…`, `logoPool…` and `logoWord…`.
struct WelcomeLogo: View {
    /// The sheep's width; the word and the glow scale with it.
    let size: CGFloat

    @Environment(\.accessibilityReduceMotion)
    private var reduceMotion

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
    private var wordWidth: CGFloat { size * Proportion.wordWidth }
    private var wordHeight: CGFloat { wordWidth / Proportion.wordAspect }

    var body: some View {
        TimelineView(.animation(paused: reduceMotion)) { timeline in
            let t = timeline.date.timeIntervalSinceReferenceDate
            VStack(spacing: size * Proportion.gap) {
                // The sheep's slot is the glow's anchor, so the glow stays
                // still while the sheep floats over it.
                Color.clear
                    .frame(width: size, height: sheepHeight)
                    .background { glow }
                    .overlay {
                        Image("Sheep")
                            .resizable()
                            .scaledToFit()
                            .modifier(
                                Bob(
                                    reduced: reduceMotion,
                                    time: t,
                                    rise: Proportion.sheepRise * sheepHeight
                                )
                            )
                    }
                Image("LogoWord")
                    .resizable()
                    .scaledToFit()
                    .foregroundStyle(Theme.logoWord)
                    .shadow(color: Theme.logoWordEdge, radius: 0, x: 1, y: 1)
                    .frame(width: wordWidth, height: wordHeight)
                    .modifier(
                        Bob(
                            reduced: reduceMotion,
                            time: t - Motion.wordDelay,
                            rise: Proportion.wordRise * wordHeight
                        )
                    )
            }
        }
    }

    /// The disc of light centred 65% down the sheep and the pool under it,
    /// which take no layout.
    private var glow: some View {
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

#if DEBUG
    #Preview("Welcome logo") {
        WelcomeLogo(size: 120)
            .padding(80)
            .windowBackground()
    }
#endif
