import Foundation

/// A line of short facts about one thing ("Vinyl · 1973 · UK"), separated by
/// the locale's fact separator from the Core catalog, so a translation can
/// choose another glyph.
public enum FactList {
    /// The separator, spaced as it sits between two facts.
    public static var separator: String {
        localizedCoreString("core.audio.list_separator")
    }

    /// The separator without its spaces, for a view that puts the glyph in a
    /// `Text` of its own between two facts.
    public static var glyph: String {
        separator.trimmingCharacters(in: .whitespaces)
    }

    /// The facts in order, leaving out missing and empty ones.
    public static func joined(_ facts: [String?]) -> String {
        facts.compactMap { $0?.isEmpty == false ? $0 : nil }
            .joined(separator: separator)
    }
}
