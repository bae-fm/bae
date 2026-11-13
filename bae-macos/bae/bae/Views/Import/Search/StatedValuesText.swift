import BaeKit
import SwiftUI

/// The values the records of a pressing state, each followed by the catalogs
/// that state it, in grey: "1992 MusicBrainz / 1993 Discogs". Each part is
/// one fact's values, and the parts follow each other under the list
/// separator.
struct StatedValuesText: View {
    let parts: [[StatedValue]]

    init(_ values: [StatedValue]) {
        parts = [values]
    }

    init(parts: [[StatedValue]]) {
        self.parts = parts
    }

    var body: some View {
        Text(attributed)
    }

    private var attributed: AttributedString {
        let alternatives = localizedCoreString(
            "core.pressing.alternatives_separator"
        )
        var text = AttributedString()
        for part in parts where !part.isEmpty {
            if !text.characters.isEmpty {
                text.append(AttributedString(FactList.separator))
            }
            for (index, stated) in part.enumerated() {
                if index > 0 {
                    text.append(grey(alternatives))
                }
                text.append(AttributedString(stated.value))
                if !stated.catalogs.isEmpty {
                    text.append(grey(" \(stated.catalogs)"))
                }
            }
        }
        return text
    }

    private func grey(_ text: String) -> AttributedString {
        var grey = AttributedString(text)
        grey.foregroundColor = Color(nsColor: .tertiaryLabelColor)
        return grey
    }
}
