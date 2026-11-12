import Foundation
import Testing

/// Left-to-right assumptions in symbols and arrows: the views name
/// `chevron.forward`/`chevron.backward` (which SwiftUI mirrors), and the arrow
/// between two values is a catalog message right-to-left languages turn.
@Suite("Direction-neutral symbols")
struct DirectionNeutralSymbolsTests {
    @Test("no view names a left or right chevron")
    func noFixedDirectionChevrons() throws {
        let views = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .appending(path: "bae/Views")
        let files = try #require(
            FileManager.default.enumerator(
                at: views,
                includingPropertiesForKeys: nil
            )
        )
        for case let url as URL in files where url.pathExtension == "swift" {
            let source = try String(contentsOf: url, encoding: .utf8)
            #expect(
                !source.contains("\"chevron.left\"")
                    && !source.contains("\"chevron.right\""),
                "\(url.lastPathComponent) names a fixed-direction chevron"
            )
        }
    }

    @Test("the arrow message points left in Arabic and Hebrew")
    func arrowMessageTurnsForRightToLeft() throws {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .appending(path: "bae/Localizable.xcstrings")
        let catalog = try #require(
            try JSONSerialization.jsonObject(with: Data(contentsOf: url))
                as? [String: Any]
        )
        let strings = try #require(catalog["strings"] as? [String: Any])
        let entry = try #require(strings["%@ → %@"] as? [String: Any])
        let localizations = try #require(
            entry["localizations"] as? [String: [String: [String: String]]]
        )
        for locale in ["ar", "he"] {
            #expect(
                localizations[locale]?["stringUnit"]?["value"]
                    == "%1$@ ← %2$@"
            )
        }
        #expect(
            localizations["fr"]?["stringUnit"]?["value"] == "%1$@ → %2$@"
        )
    }
}
