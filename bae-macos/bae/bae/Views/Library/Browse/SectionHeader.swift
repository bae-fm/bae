import BaeKit
import SwiftUI

/// The heading ("Works", "Releases", "Recordings", "Credits") above a group of
/// detail-pane rows.
struct SectionHeader: View {
    let title: String

    var body: some View {
        Text(title)
            .themeText(.heading)
            .padding(.top, 4)
    }
}

#if DEBUG
    #Preview("Section Header") {
        VStack(alignment: .leading, spacing: 12) {
            SectionHeader(title: "Works")
            SectionHeader(title: "Releases")
            SectionHeader(title: "Recordings")
        }
        .padding()
        .frame(width: 320, alignment: .leading)
    }
#endif
