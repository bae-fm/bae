import BaeKit
import SwiftUI

/// A pressing fact: the values the records of a pressing state, each followed
/// by the catalogs that state it, in grey: "1992 MusicBrainz / 1993 Discogs".
/// Each part is one fact's values, and the parts follow each other under the
/// list separator.
struct MacSourcedValue: View {
    let values: [StatedValue]

    init(_ values: [StatedValue]) {
        self.values = values
    }

    var body: some View {
        Text(attributed)
    }

    var attributed: AttributedString {
        let alternatives = localizedCoreString(
            "core.pressing.alternatives_separator"
        )
        var text = AttributedString()
        for (index, stated) in values.enumerated() {
            if index > 0 {
                text.append(grey(alternatives))
            }
            text.append(AttributedString(stated.value))
            if !stated.catalogs.isEmpty {
                text.append(grey(" \(stated.catalogs)"))
            }
        }
        return text
    }

    private func grey(_ text: String) -> AttributedString {
        var grey = AttributedString(text)
        grey.foregroundColor = Theme.textTertiary
        return grey
    }
}
