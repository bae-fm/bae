import SwiftUI

/// The sheep's own inks, which no theme changes, and the line it is drawn in.
enum SheepInk {
    static let plum = Color(
        red: 0x40 / 255,
        green: 0x18 / 255,
        blue: 0x3D / 255
    )
    static let cream = Color(
        red: 0xFF / 255,
        green: 0xFC / 255,
        blue: 0xF4 / 255
    )
    static let tongue = Color(
        red: 0xFF / 255,
        green: 0x81 / 255,
        blue: 0xBB / 255
    )
    static let blush = Color(
        red: 0xE0 / 255,
        green: 0x50 / 255,
        blue: 0x8F / 255
    )
    static let cdSilver = Color(
        red: 0xE9 / 255,
        green: 0xE5 / 255,
        blue: 0xF4 / 255
    )

    /// The outline's stroke: round at the ends and corners.
    static func line(_ width: Double) -> StrokeStyle {
        StrokeStyle(lineWidth: width, lineCap: .round, lineJoin: .round)
    }

    /// `path` filled with `fill` and outlined in plum `width` wide.
    static func fillAndLine(
        _ path: Path,
        fill: Color,
        width: Double,
        evenOdd: Bool = false,
        in context: GraphicsContext
    ) {
        context.fill(
            path,
            with: .color(fill),
            style: FillStyle(eoFill: evenOdd)
        )
        context.stroke(path, with: .color(plum), style: line(width))
    }
}
