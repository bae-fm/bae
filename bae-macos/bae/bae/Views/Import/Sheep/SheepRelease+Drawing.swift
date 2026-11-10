import SwiftUI

extension SheepRelease.Kind {
    /// A release in `tone` drawn to fit a circle of radius 50 around the
    /// origin, in the sheep's line: a cover, a vinyl record, a CD, or a record
    /// half out of its sleeve.
    func draw(tone: ReleaseTone, in context: GraphicsContext) {
        switch self {
        case .vinyl:
            Self.drawVinyl(tone: tone, in: context)
        case .cd:
            Self.drawCD(tone: tone, in: context)
        case .cover:
            Self.drawCover(tone: tone, in: context)
        case .sleeve(let disc):
            var record = context
            record.translateBy(x: 24, y: 0)
            record.scaleBy(x: 0.9, y: 0.9)
            Self.drawVinyl(tone: disc, in: record)
            var cover = context
            cover.translateBy(x: -12, y: 0)
            Self.drawCover(tone: tone, in: cover)
        }
    }

    private static func drawVinyl(
        tone: ReleaseTone,
        in context: GraphicsContext
    ) {
        SheepInk.fillAndLine(
            SheepShapes.disc,
            fill: tone.color,
            width: 9,
            evenOdd: true,
            in: context
        )
        for radius in [40.0, 31.0] {
            context.stroke(
                Path(
                    ellipseIn: CGRect(
                        x: -radius,
                        y: -radius,
                        width: 2 * radius,
                        height: 2 * radius
                    )
                ),
                with: .color(SheepInk.plum.opacity(0.3)),
                style: SheepInk.line(3)
            )
        }
        context.stroke(
            SheepShapes.shine,
            with: .color(.white.opacity(0.6)),
            style: SheepInk.line(6)
        )
        context.fill(
            SheepShapes.label,
            with: .color(SheepInk.cream),
            style: FillStyle(eoFill: true)
        )
        context.stroke(
            SheepShapes.label,
            with: .color(SheepInk.plum),
            style: SheepInk.line(4)
        )
    }

    private static func drawCD(tone: ReleaseTone, in context: GraphicsContext) {
        SheepInk.fillAndLine(
            SheepShapes.cdDisc,
            fill: SheepInk.cdSilver,
            width: 9,
            evenOdd: true,
            in: context
        )
        context.stroke(
            SheepShapes.cdSkyArc,
            with: .color(ReleaseTone.sky.color),
            style: SheepInk.line(9)
        )
        context.stroke(
            SheepShapes.cdToneArc,
            with: .color(tone.color),
            style: SheepInk.line(9)
        )
        context.stroke(
            Path(ellipseIn: CGRect(x: -19, y: -19, width: 38, height: 38)),
            with: .color(SheepInk.plum.opacity(0.35)),
            style: SheepInk.line(4)
        )
    }

    private static func drawCover(
        tone: ReleaseTone,
        in context: GraphicsContext
    ) {
        SheepInk.fillAndLine(
            Path(
                roundedRect: CGRect(x: -50, y: -50, width: 100, height: 100),
                cornerSize: CGSize(width: 10, height: 10)
            ),
            fill: tone.color,
            width: 9,
            in: context
        )
        context.fill(
            Path(ellipseIn: CGRect(x: -3, y: -29, width: 34, height: 34)),
            with: .color(.white.opacity(0.5))
        )
        context.stroke(
            SheepShapes.coverWave,
            with: .color(SheepInk.plum.opacity(0.3)),
            style: SheepInk.line(7)
        )
    }
}
