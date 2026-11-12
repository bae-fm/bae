import BaeKit
import SwiftUI

/// A port number for a server bae runs: a field five digits wide, at the
/// row's trailing edge, that takes digits only. It saves on Return or on
/// leaving the field, and takes what is wrong with it from its row, which says
/// so on its second line.
struct MacPortField: View {
    /// The port as stored.
    let value: String
    /// The server can't use this port: the field's edge turns red.
    var invalid = false
    /// Send the typed port to wherever it lives.
    let onCommit: @MainActor (String) async -> Void

    /// The most digits a port has.
    private static let digits = 5

    /// Five digits wide, as the widest digits set.
    static let width: CGFloat = {
        let widest = String(repeating: "0", count: digits) as NSString
        let text = widest.size(withAttributes: [.font: ThemeText.body.nsFont])
        return ceil(text.width) + 2 * ThemeSpace.fieldInset + 4
    }()

    /// What typing leaves of the text: the first five digits.
    static func digitsOnly(_ text: String) -> String {
        String(text.filter(\.isASCII).filter(\.isNumber).prefix(digits))
    }

    var body: some View {
        CommittedTextField(
            placeholder: "",
            value: value,
            fillsWidth: true,
            invalid: invalid,
            filter: Self.digitsOnly,
            onCommit: onCommit
        )
        .frame(width: Self.width)
        .accessibilityLabel(Text("Port"))
    }
}

#if DEBUG
    #Preview("Port field") {
        Form {
            LabeledContent("Port") {
                MacPortField(value: "4533", onCommit: { _ in })
            }
            LabeledContent("Port") {
                MacPortField(value: "4533", invalid: true, onCommit: { _ in })
            }
        }
        .formStyle(.grouped)
        .frame(width: 400, height: 160)
    }
#endif
