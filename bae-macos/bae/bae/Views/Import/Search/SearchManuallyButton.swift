import BaeKit
import SwiftUI

/// Opens the search with the cursor in its first field, for a section with
/// nothing to pick from.
struct SearchManuallyButton: View {
    let action: () -> Void

    var body: some View {
        MacButton("Search manually", action: action)
    }
}
