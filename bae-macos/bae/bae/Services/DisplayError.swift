import Foundation

/// A typed core failure that renders to a localized line plus optional opaque
/// detail. Both `BridgeError` and `BridgePlaybackErrorReason` satisfy it (they
/// expose `localizedLine`/`detail` in `BridgeError+Localized`), so a
/// `DisplayError` builds from either uniformly.
protocol LocalizedFailure {
    var localizedLine: String { get }
    var detail: String? { get }
}

extension BridgeError: LocalizedFailure {}
extension BridgePlaybackErrorReason: LocalizedFailure {}

/// A user-facing error ready to render: a localized line plus, for diagnostics,
/// the opaque Rust error chain to offer in a copyable disclosure (never
/// translated). UI-originated errors (a failed drop, a caught Swift error) are
/// already localized prose and carry no detail; core errors crossing the bridge
/// arrive as typed reasons and render their generic per-category line here.
struct DisplayError: Equatable {
    let line: String
    let detail: String?

    /// A UI-originated error: prose the UI already localized, with no opaque
    /// detail to disclose.
    init(line: String) {
        self.line = line
        self.detail = nil
    }

    /// A typed core failure crossing the bridge — renders its generic
    /// per-category line, with the opaque Rust detail offered separately.
    init(_ failure: LocalizedFailure) {
        self.line = failure.localizedLine
        self.detail = failure.detail
    }
}
