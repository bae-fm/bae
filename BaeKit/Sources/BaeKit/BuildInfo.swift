import Foundation

public enum AppEdition: String, Sendable {
    case bae
    case baeium
}

/// Info.plist values shared by diagnostics and crash-reporting setup. Values
/// may be missing or left as unsubstituted `$(...)` placeholders in dev builds.
enum BuildInfo {
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
