import BaeKit
import SwiftUI
import Testing

/// A filled notice draws its secondary words as the primary text at 72%
/// opacity, so they take the fill's tint, as the boards draw them; a quiet
/// notice, which has no fill, keeps the screen's secondary text.
@MainActor
@Suite("Notice secondary text")
struct NoticeSecondaryTextTests {
    private static func resolved(_ color: Color) -> Color.Resolved {
        var environment = EnvironmentValues()
        environment.colorScheme = .dark
        return color.resolve(in: environment)
    }

    @Test("a filled notice's secondary words are the primary text at 72%")
    func filledTakesTheTint() {
        let words = Self.resolved(NoticeLook.filled.secondaryText)
        let primary = Self.resolved(Theme.textPrimary)
        #expect(abs(words.opacity - 0.72 * primary.opacity) < 0.005)
        #expect(words.red == primary.red)
        #expect(words.green == primary.green)
        #expect(words.blue == primary.blue)
    }

    @Test("a quiet notice's secondary words are the plain secondary text")
    func quietKeepsSecondary() {
        let words = Self.resolved(NoticeLook.quiet.secondaryText)
        let secondary = Self.resolved(Theme.textSecondary)
        #expect(words.red == secondary.red)
        #expect(words.green == secondary.green)
        #expect(words.blue == secondary.blue)
        #expect(words.opacity == secondary.opacity)
    }
}
