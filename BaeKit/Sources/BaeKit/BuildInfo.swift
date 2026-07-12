import Foundation

/// Build metadata shared by diagnostics and crash-reporting setup: the
/// edition compiled in, and Info.plist values that may be missing or left as
/// unsubstituted `$(...)` placeholders in dev builds.
enum BuildInfo {
    static var edition: String {
        #if BAE_OAUTH_PROVIDERS
            "bae"
        #else
            "baeium"
        #endif
    }

    /// The trimmed Info.plist string for `key`, or nil when absent, empty, or
    /// an unsubstituted `$(...)` placeholder.
    static func infoString(_ key: String) -> String? {
        guard let value = Bundle.main.infoDictionary?[key] as? String else {
            return nil
        }
        let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
        if trimmed.isEmpty || trimmed.hasPrefix("$(") {
            return nil
        }
        return trimmed
    }
}
