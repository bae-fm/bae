import SwiftUI

/// The bold section label ("Works", "Releases", "Recordings", "Credits") above
/// a group of detail-pane rows.
struct SectionHeader: View {
    let title: String

    var body: some View {
        Text(title)
            .font(.browseSectionLabel)
            .padding(.top, 4)
    }
}
