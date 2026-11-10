import SwiftUI

/// What the sheep does around itself: the releases it takes in, the streaks
/// of air of its breath in, the marks of surprise, the stars and the heart.
extension ImportSheepPainter {
    func drawRelease(_ index: Int, in context: GraphicsContext) {
        let release = moment.releases[index]
        moved(.release(index), around: .zero, context) { context in
            moved(.spin(index), around: .zero, context) { context in
                context.scaleBy(x: release.radius / 50, y: release.radius / 50)
                release.kind.draw(tone: release.tone, in: context)
            }
        }
    }

    /// Streaks of air for the breath in.
    func drawAir(_ context: GraphicsContext) {
        for index in 0..<4 {
            moved(.wind(index), around: .zero, context) {
                $0.stroke(
                    SheepShapes.wind,
                    with: .color(Theme.accent),
                    style: SheepInk.line(13)
                )
            }
        }
    }

    /// Marks of surprise beside the head, on the side a release comes from,
    /// or above it.
    func drawSurprise(_ context: GraphicsContext) {
        let marks: [(side: SheepPart.Side, path: Path)] = [
            (.right, SheepShapes.surpriseRight),
            (.left, SheepShapes.surpriseLeft),
            (.top, SheepShapes.surpriseTop),
        ]
        for mark in marks {
            let pivot =
                switch mark.side {
                case .right: CGPoint(x: 480, y: 30)
                case .left: CGPoint(x: 0, y: 30)
                case .top: CGPoint(x: 240, y: -40)
                }
            moved(.surprise(mark.side), around: pivot, context) {
                $0.stroke(
                    mark.path,
                    with: .color(Theme.accent),
                    style: SheepInk.line(14)
                )
            }
        }
    }

    /// The stars that burst out after a catch, and the heart that floats up.
    func drawStars(_ context: GraphicsContext) {
        for (index, star) in Self.stars.enumerated() {
            var placed = context
            placed.translateBy(x: star.at.x, y: star.at.y)
            placed.scaleBy(x: star.scale, y: star.scale)
            moved(.star(index), around: .zero, placed) {
                SheepInk.fillAndLine(
                    SheepShapes.star,
                    fill: (index % 2 == 1 ? ReleaseTone.gold : .pink).color,
                    width: 7,
                    in: $0
                )
            }
        }
        moved(.floatingHeart, around: .zero, context) {
            SheepInk.fillAndLine(
                SheepShapes.floatingHeart,
                fill: ReleaseTone.pink.color,
                width: 8,
                in: $0
            )
        }
    }

    private static let stars: [(at: CGPoint, scale: Double)] = [
        (CGPoint(x: 500, y: 110), 1.5), (CGPoint(x: -30, y: 40), 1.2),
        (CGPoint(x: 590, y: 330), 1.1), (CGPoint(x: -70, y: 310), 1.35),
        (CGPoint(x: 250, y: -150), 1),
    ]
}
