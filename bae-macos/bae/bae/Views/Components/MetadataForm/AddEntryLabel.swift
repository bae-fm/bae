import BaeKit
import SwiftUI

/// The words every list of the release editor adds an entry with: a plus and
/// the words, in the accent.
struct AddEntryLabel: View {
    let words: String

    var body: some View {
        HStack(spacing: ThemeSpace.line) {
            Image(systemName: "plus")
                .themeIcon(.small)
            Text(verbatim: words)
                .themeText(.body)
        }
        .foregroundStyle(Theme.accent)
    }
}
