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
}
