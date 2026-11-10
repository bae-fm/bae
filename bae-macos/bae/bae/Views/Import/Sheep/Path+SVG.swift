import SwiftUI

extension Path {
    /// The path an SVG `d` attribute describes, in the SVG's own coordinates.
    ///
    /// Reads `M L H V C Q A Z` and their lowercase (relative) forms, which
    /// are all the sheep's drawings use; anything else is a mistake in the
    /// data, which stops the app so it can't draw a wrong shape unseen.
    init(svg data: String) {
        var reader = SVGPathReader(data)
        var command: Character = "M"
        var drawing = SVGPathDrawing()
        while true {
            switch reader.nextToken() {
            case .end:
                self = drawing.path
                return
            case .command(let letter):
                command = letter
            case .numbers:
                precondition(
                    command.uppercased() != "Z",
                    "SVG path data has numbers after a close: \(data)"
                )
            }
            command = drawing.apply(command, reading: &reader)
        }
    }
}

/// A path being drawn from SVG commands, with the point the pen is at.
private struct SVGPathDrawing {
    var path = Path()
    private var current = CGPoint.zero
    private var subpathStart = CGPoint.zero

    /// Draws `command` with the numbers `reader` has next, and returns the
    /// command that continues if more numbers follow with no letter: a move
    /// goes on as a line.
    mutating func apply(
        _ command: Character,
        reading reader: inout SVGPathReader
    )
        -> Character
    {
        let relative = command.isLowercase
        let origin = relative ? current : .zero
        switch command.uppercased() {
        case "M", "L", "H", "V":
            lineOrMove(command, from: origin, &reader)
            return command == "M" ? "L" : command == "m" ? "l" : command
        case "C", "Q", "A":
            curve(command, from: origin, &reader)
        case "Z":
            path.closeSubpath()
            current = subpathStart
        default:
            preconditionFailure("unsupported SVG path command \(command)")
        }
        return command
    }

    private mutating func lineOrMove(
        _ command: Character,
        from origin: CGPoint,
        _ reader: inout SVGPathReader
    ) {
        switch command.uppercased() {
        case "M":
            current = reader.point(from: origin)
            subpathStart = current
            path.move(to: current)
        case "H":
            current.x = reader.number() + origin.x
            path.addLine(to: current)
        case "V":
            current.y = reader.number() + origin.y
            path.addLine(to: current)
        default:
            current = reader.point(from: origin)
            path.addLine(to: current)
        }
    }

    private mutating func curve(
        _ command: Character,
        from origin: CGPoint,
        _ reader: inout SVGPathReader
    ) {
        switch command.uppercased() {
        case "C":
            let first = reader.point(from: origin)
            let second = reader.point(from: origin)
            current = reader.point(from: origin)
            path.addCurve(to: current, control1: first, control2: second)
        case "Q":
            let control = reader.point(from: origin)
            current = reader.point(from: origin)
            path.addQuadCurve(to: current, control: control)
        default:
            let radii = CGSize(width: reader.number(), height: reader.number())
            let tilt = reader.number()
            let large = reader.number() != 0
            let sweep = reader.number() != 0
            let end = reader.point(from: origin)
            let arc = SVGArc(
                start: current,
                end: end,
                radii: radii,
                tiltDegrees: tilt,
                large: large,
                sweep: sweep
            )
            for cubic in arc.cubics {
                path.addCurve(
                    to: cubic.end,
                    control1: cubic.control1,
                    control2: cubic.control2
                )
            }
            current = end
        }
    }
}
