import BaeKit
import SwiftUI

/// A not-yet-loaded row's cell: a placeholder bar in the release column,
/// empty elsewhere.
struct StorageRowPlaceholderCell: View {
    let column: StorageTableColumn

    var body: some View {
        Group {
            if column == .release {
                RoundedRectangle(cornerRadius: ThemeRadius.bar)
                    .fill(Theme.placeholder)
                    .frame(width: 180, height: 14)
            }
            else {
                EmptyView()
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.horizontal, ThemeSpace.inline)
    }
}
