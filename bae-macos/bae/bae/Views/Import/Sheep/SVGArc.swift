import SwiftUI

/// An SVG elliptical arc from `start` to `end`, which SwiftUI draws as cubic
/// curves, none turning more than a quarter (SVG implementation notes, F.6.5
/// and F.6.6).
struct SVGArc {
    struct Cubic {
        let control1: CGPoint
        let control2: CGPoint
        let end: CGPoint
    }

    let start: CGPoint
    let end: CGPoint
    let radii: CGSize
    let tiltDegrees: Double
    let large: Bool
    let sweep: Bool

    /// The curves that make up the arc, none when its ends are one point and
    /// a straight line when a radius is zero.
    var cubics: [Cubic] {
        guard start != end else { return [] }
        guard radii.width != 0, radii.height != 0 else {
            return [Cubic(control1: start, control2: end, end: end)]
        }
        let ellipse = Ellipse(arc: self)
        let pieces = Int(
            (abs(ellipse.sweepAngle) / (.pi / 2) - 1e-9).rounded(.up)
        )
        let step = ellipse.sweepAngle / Double(max(pieces, 1))
        // How far along the tangent a quarter of a turn's control point lies.
        let handle = 4.0 / 3.0 * tan(step / 4)
        return (0..<max(pieces, 1))
            .map { piece in
                let from = ellipse.startAngle + Double(piece) * step
                let to = from + step
                let leave = ellipse.slope(at: from)
                let arrive = ellipse.slope(at: to)
                let first = ellipse.point(at: from)
                let last = ellipse.point(at: to)
                return Cubic(
                    control1: CGPoint(
                        x: first.x + handle * leave.x,
                        y: first.y + handle * leave.y
                    ),
                    control2: CGPoint(
                        x: last.x - handle * arrive.x,
                        y: last.y - handle * arrive.y
                    ),
                    end: piece == max(pieces, 1) - 1 ? end : last
                )
            }
    }

    /// The arc's ellipse: its centre, radii (grown when too small to reach
    /// both ends), tilt, and the angles the arc runs between.
    private struct Ellipse {
        let centre: CGPoint
        let rx: Double
        let ry: Double
        let cosTilt: Double
        let sinTilt: Double
        let startAngle: Double
        let sweepAngle: Double

        init(arc: SVGArc) {
            let tilt = arc.tiltDegrees * .pi / 180
            cosTilt = cos(tilt)
            sinTilt = sin(tilt)
            var rx = abs(arc.radii.width)
            var ry = abs(arc.radii.height)
            let halfX = (arc.start.x - arc.end.x) / 2
            let halfY = (arc.start.y - arc.end.y) / 2
            let x1 = cosTilt * halfX + sinTilt * halfY
            let y1 = -sinTilt * halfX + cosTilt * halfY
            let overreach = (x1 * x1) / (rx * rx) + (y1 * y1) / (ry * ry)
            if overreach > 1 {
                rx *= overreach.squareRoot()
                ry *= overreach.squareRoot()
            }
            let numerator =
                rx * rx * ry * ry - rx * rx * y1 * y1 - ry * ry * x1 * x1
            let denominator = rx * rx * y1 * y1 + ry * ry * x1 * x1
            var scale = (max(0, numerator) / denominator).squareRoot()
            if arc.large == arc.sweep { scale = -scale }
            let centreX1 = scale * rx * y1 / ry
            let centreY1 = -scale * ry * x1 / rx
            centre = CGPoint(
                x: cosTilt * centreX1 - sinTilt * centreY1
                    + (arc.start.x + arc.end.x) / 2,
                y: sinTilt * centreX1 + cosTilt * centreY1
                    + (arc.start.y + arc.end.y) / 2
            )
            self.rx = rx
            self.ry = ry
            startAngle = atan2((y1 - centreY1) / ry, (x1 - centreX1) / rx)
            let endAngle = atan2((-y1 - centreY1) / ry, (-x1 - centreX1) / rx)
            var sweepAngle = endAngle - startAngle
            if arc.sweep, sweepAngle < 0 {
                sweepAngle += 2 * .pi
            }
            else if !arc.sweep, sweepAngle > 0 {
                sweepAngle -= 2 * .pi
            }
            self.sweepAngle = sweepAngle
        }

        func point(at angle: Double) -> CGPoint {
            let alongX = rx * cos(angle)
            let alongY = ry * sin(angle)
            return CGPoint(
                x: centre.x + alongX * cosTilt - alongY * sinTilt,
                y: centre.y + alongX * sinTilt + alongY * cosTilt
            )
        }

        func slope(at angle: Double) -> CGPoint {
            let alongX = -rx * sin(angle)
            let alongY = ry * cos(angle)
            return CGPoint(
                x: alongX * cosTilt - alongY * sinTilt,
                y: alongX * sinTilt + alongY * cosTilt
            )
        }
    }
}
