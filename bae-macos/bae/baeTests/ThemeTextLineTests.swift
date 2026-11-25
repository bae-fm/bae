import AppKit
import BaeKit
import SwiftUI
import Testing

/// Each line of multi-line text in a role that names a line height is that
/// tall, as the boards draw it.
@MainActor
@Suite("Theme text lines")
struct ThemeTextLineTests {
    private func height(of role: ThemeText, lines: Int) throws -> CGFloat {
        let text = Array(repeating: "Line", count: lines)
            .joined(separator: "\n")
        let host = NSHostingView(
            rootView: Text(verbatim: text)
                .themeText(role)
                .fixedSize()
        )
        return host.fittingSize.height
    }

    @Test(
        "lines are as tall as the role's line height",
        arguments: [ThemeText.detail, .body, .strong, .alertMessage]
    )
    func linesAreTheRolesHeight(role: ThemeText) throws {
        // A role with no line height keeps its font's own.
        guard let line = role.macOSLine else { return }
        for lines in 1...4 {
            let height = try height(of: role, lines: lines)
            #expect(
                abs(height - line * CGFloat(lines)) < 0.01,
                "\(lines) lines are \(height) high, not \(line * CGFloat(lines))"
            )
        }
    }

    /// The y of the bottom of an "H" set in the detail role, in points from
    /// the top of its line: where the role's baseline sits.
    private func detailBaseline() throws -> CGFloat {
        let host = NSHostingView(
            rootView: Text(verbatim: "H")
                .themeText(.detail)
                .foregroundStyle(.black)
                .fixedSize()
        )
        host.frame = CGRect(origin: .zero, size: host.fittingSize)
        host.layoutSubtreeIfNeeded()
        let rep = try #require(
            host.bitmapImageRepForCachingDisplay(in: host.bounds)
        )
        host.cacheDisplay(in: host.bounds, to: rep)
        let scale = CGFloat(rep.pixelsHigh) / host.bounds.height
        var bottom: Int?
        for row in 0..<rep.pixelsHigh {
            for column in 0..<rep.pixelsWide {
                let color = try #require(rep.colorAt(x: column, y: row))
                if color.alphaComponent > 0.5 {
                    bottom = row
                    break
                }
            }
        }
        return (CGFloat(try #require(bottom)) + 1) / scale
    }

    @Test("detail text's baseline is 12 points down its 15 point line")
    func detailTextSitsWhereTheBoardDrawsIt() throws {
        // The board's line puts the spare height above the words: the
        // 11.5 point text's baseline is 12 points down the 15 point line.
        let baseline = try detailBaseline()
        #expect(
            abs(baseline - 12) < 0.5,
            "the baseline is \(baseline) points down, not 12"
        )
    }
}
