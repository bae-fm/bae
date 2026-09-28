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
                Image(systemName: "chevron.right")
                    .themeIcon(.small)
                    .rotationEffect(.degrees(isExpanded ? 90 : 0))
                if isExpanded {
                    Text("Show fewer")
                }
                else {
                    Text("\(Int(count)) more releases")
                }
            }
            .themeText(.body)
            .foregroundStyle(.secondary)
            .contentShape(.rect)
        }
        .buttonStyle(.plain)
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}
