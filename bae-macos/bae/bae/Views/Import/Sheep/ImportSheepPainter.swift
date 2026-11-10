import SwiftUI

/// Draws the sheep and the releases it is taking in at one moment of its loop
/// (`time` seconds into `moment`), in the drawing's units: the sheep's box is
/// 584 by 545 at the origin, inside a wider and taller view box that leaves
/// room for the releases on their way in.
struct ImportSheepPainter {
    /// The drawing's view box: the sheep's box with room around it.
    static let viewBox = CGRect(x: -170, y: -215, width: 924, height: 767)

    let moment: SheepMoment
    let time: Double

    /// Draws into a canvas of `size`, scaling the view box to fill it.
    func draw(into context: inout GraphicsContext, size: CGSize) {
        let scale = size.width / Self.viewBox.width
        context.scaleBy(x: scale, y: scale)
        context.translateBy(x: -Self.viewBox.minX, y: -Self.viewBox.minY)
        drawSheep(context)
        for index in 0..<moment.releases.count {
            drawRelease(index, in: context)
        }
        drawAir(context)
        drawSurprise(context)
        drawStars(context)
    }

    // MARK: The sheep

    /// The pivots the parts turn and scale around, in the drawing's units.
    enum Pivot {
        static let roll = CGPoint(x: 292, y: 300)
        static let rig = CGPoint(x: 292, y: 540)
        static let head = CGPoint(x: 245, y: 330)
        static let earLeft = CGPoint(x: 100, y: 190)
        static let earRight = CGPoint(x: 395, y: 258)
        static let features = CGPoint(x: 230, y: 270)
        static let cheekLeft = CGPoint(x: 124, y: 267)
        static let cheekRight = CGPoint(x: 338, y: 277)
        static let mouth = CGPoint(x: 226, y: 266)
        static let munch = CGPoint(x: 226, y: 276)
        static let eyes = CGPoint(x: 230, y: 248)
        static let blush = CGPoint(x: 230, y: 275)
        static let heart = CGPoint(x: 220, y: 398)
        static let arms = CGPoint(x: 222, y: 470)
    }

    private func drawSheep(_ context: GraphicsContext) {
        moved(.roll, around: Pivot.roll, context) { context in
            moved(.rig, around: Pivot.rig, context) { context in
                moved(.head, around: Pivot.head, context) { context in
                    draw(.headband, in: context)
                    draw(.earcupLeft, in: context)
                }
                draw(.body, in: context)
                moved(.head, around: Pivot.head, context) { drawHead($0) }
                moved(.arms, around: Pivot.arms, context) { context in
                    draw(.player, in: context)
                    // The screen lighting up as a caught release goes in.
                    moved(.flash, around: .zero, context) {
                        draw(.screenGlow, in: $0)
                    }
                    moved(.heart, around: Pivot.heart, context) {
                        draw(.heart, in: $0)
                    }
                    draw(.bubbles, in: context)
                    draw(.handLeft, in: context)
                    draw(.handRight, in: context)
                }
            }
        }
    }

    private func drawHead(_ context: GraphicsContext) {
        moved(.earLeft, around: Pivot.earLeft, context) {
            draw(.earLeft, in: $0)
        }
        moved(.earRight, around: Pivot.earRight, context) {
            draw(.earRight, in: $0)
        }
        draw(.face, in: context)
        // The features stay inside the face as they slide with its turn.
        context.drawLayer { layer in
            moved(.features, around: Pivot.features, layer) {
                drawFeatures($0)
            }
            layer.blendMode = .destinationIn
            draw(.faceClip, in: layer)
        }
        draw(.wool, in: context)
        draw(.earcupRight, in: context)
    }

    private func drawFeatures(_ context: GraphicsContext) {
        moved(.cheekLeft, around: Pivot.cheekLeft, context) {
            draw(.cheekLeft, in: $0)
        }
        moved(.cheekRight, around: Pivot.cheekRight, context) {
            draw(.cheekRight, in: $0)
        }
        // Blush lines across the cheeks, for after a catch.
        moved(.blush, around: Pivot.blush, context) {
            $0.stroke(
                SheepShapes.blush,
                with: .color(SheepInk.blush),
                style: SheepInk.line(6)
            )
        }
        moved(.mouth, around: Pivot.mouth, context) { draw(.mouth, in: $0) }
        // The mouth opened wide to chomp, in the sheep's own mouth colours,
        // and the closed mouth that munches.
        moved(.gape, around: Pivot.mouth, context) { gape in
            SheepInk.fillAndLine(
                SheepShapes.gape,
                fill: SheepInk.plum,
                width: 6,
                in: gape
            )
            gape.fill(SheepShapes.tongue, with: .color(SheepInk.tongue))
        }
        moved(.munch, around: Pivot.munch, context) {
            $0.stroke(
                SheepShapes.munch,
                with: .color(SheepInk.plum),
                style: SheepInk.line(11)
            )
        }
        moved(.eyes, around: Pivot.eyes, context) { draw(.eyes, in: $0) }
    }

    // MARK: Helpers

    private func draw(_ art: SheepArt, in context: GraphicsContext) {
        context.draw(
            context.resolve(Image(art.assetName)),
            in: CGRect(origin: .zero, size: SheepArt.box)
        )
    }

    /// Draws `body` in a copy of `context` placed as `part`'s pose places it,
    /// turning and scaling around `pivot`, and not at all while the part is
    /// unseen.
    func moved(
        _ part: SheepPart,
        around pivot: CGPoint,
        _ context: GraphicsContext,
        _ body: (inout GraphicsContext) -> Void
    ) {
        let pose = moment.pose(of: part, at: time)
        guard pose.opacity > 0, pose.scaleX != 0, pose.scaleY != 0 else {
            return
        }
        var inner = context
        inner.opacity = context.opacity * pose.opacity
        inner.translateBy(x: pivot.x + pose.x, y: pivot.y + pose.y)
        inner.rotate(by: .degrees(pose.turn))
        inner.scaleBy(x: pose.scaleX, y: pose.scaleY)
        inner.translateBy(x: -pivot.x, y: -pivot.y)
        body(&inner)
    }
}
