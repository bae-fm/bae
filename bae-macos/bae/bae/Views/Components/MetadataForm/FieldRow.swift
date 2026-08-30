import SwiftUI

/// A label-left / value-right row spec for a grouped card. All album and
/// pressing fields are raw `String` text, so one spec drives every row.
struct FieldRow {
    let label: String
    var hint: String?
    let placeholder: String
    /// The stored value the row shows.
    let text: String
    /// Send what was typed to wherever the value lives.
    let onCommit: @MainActor (String) async -> Void
    let width: FieldWidth
    var monospaced: Bool = false
}

enum FieldWidth {
    case short
    case medium
    case long

    var maxWidth: CGFloat {
        switch self {
        case .short:
            160
        case .medium:
            300
        case .long:
            520
        }
    }
}
