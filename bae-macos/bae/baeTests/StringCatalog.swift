import Foundation
import Testing

/// The app's source files that tests read, which the test bundle holds as
/// resources so that a test finds them wherever the build ran: Swift's
/// `#filePath` is rewritten when the compile cache maps paths.
enum StringCatalog {
    private final class BundleToken {}

    /// The bundle of the baeTests target.
    private static var bundle: Bundle { Bundle(for: BundleToken.self) }

    /// The catalog's `strings` table, from `Localizable.xcstrings`.
    static func strings() throws -> [String: Any] {
        let url = try #require(
            bundle.url(forResource: "Localizable", withExtension: "xcstrings")
        )
        let catalog = try #require(
            try JSONSerialization.jsonObject(with: Data(contentsOf: url))
                as? [String: Any]
        )
        return try #require(catalog["strings"] as? [String: Any])
    }

    /// The folder of the app's view sources.
    static func viewSources() throws -> URL {
        try #require(bundle.url(forResource: "Views", withExtension: nil))
    }
}
