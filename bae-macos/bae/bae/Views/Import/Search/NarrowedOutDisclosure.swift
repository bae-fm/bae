import BaeKit
import SwiftUI

/// The line closing a list whose agreement set rows aside: closed, it counts
/// them; open, every one of them is on its album's card above it, and it
/// offers to put them away again.
struct NarrowedOutDisclosure: View {
    let count: UInt32
    @Binding
    var isExpanded: Bool

    var body: some View {
        Button {
            isExpanded = !isExpanded
        } label: {
            HStack(spacing: ThemeSpace.inline) {
                DisclosureChevron(isExpanded: isExpanded)
                    .themeIcon(.small)
                Text(verbatim: Self.title(count: count, isExpanded: isExpanded))
            }
            .themeText(.body)
            .foregroundStyle(.secondary)
            .contentShape(.rect)
        }
        .buttonStyle(.plain)
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    /// The line's words: how many rows were set aside while they are put
    /// away, and the way to put them away while they show.
    static func title(count: UInt32, isExpanded: Bool) -> String {
        isExpanded
            ? String(localized: "Show fewer")
            : String(localized: "\(Int(count)) more releases")
    }
}
