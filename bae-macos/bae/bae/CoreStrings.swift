import Foundation

/// A message from the generated `Core` string table — the `core.*` wording core
/// itself owns, and the `ui.*` chrome shared with the other platform UI, both
/// emitted from `bae-bridge/loc/catalog.toml`. Keys come from core (a
/// `bridge_*_key` function) or are named literally where the UI composes the
/// sentence; the UI never invents the wording.
///
/// `localizedStringWithFormat` rather than `String(format:)`, so a pluralized
/// message picks its category from the locale's rules.
func coreString(_ key: String, _ argument: CVarArg? = nil) -> String {
    let format = NSLocalizedString(
        key,
        tableName: "Core",
        bundle: .main,
        comment: ""
    )
    guard let argument else { return format }
    return String.localizedStringWithFormat(format, argument)
}
