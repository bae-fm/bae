import Foundation

/// A message from the generated `Core` string table — the `core.*` wording core
/// itself owns, and the `ui.*` chrome shared with the other platform UI, both
/// emitted from `bae-bridge/loc/catalog.toml`. Keys come from core (a
/// `bridge_*_key` function) or are named literally where the UI composes the
/// sentence; the UI never invents the wording.
///
/// Arguments are **positional and ordered by the message's English value**.
/// `bae-loc`'s Apple emitter (`bae-loc/src/emit.rs`, `apple_flat`) numbers a
/// message's `%N$` specifiers by the order its arguments first appear in the
/// value it is rendering, and every translation in the catalog keeps the
/// English order — so `core.import.reconciliation.more_files` takes `files`
/// then `tracks`, and `core.import.becomes.slots` takes `first` then `last`.
///
/// A message whose value names one argument twice needs that argument passed
/// twice: Apple's plural variations carry one specifier number, so a repeated
/// `%lld` inside a variation reads the next argument rather than repeating the
/// count. The catalog holds no such message today.
///
/// `String(format:locale:)` against `Locale.current` rather than the plain
/// initializer, so a pluralized message picks its category from the locale's
/// rules.
func coreString(_ key: String, _ arguments: CVarArg...) -> String {
    let format = NSLocalizedString(
        key,
        tableName: "Core",
        bundle: .main,
        comment: ""
    )
    guard !arguments.isEmpty else { return format }
    return String(format: format, locale: Locale.current, arguments: arguments)
}
