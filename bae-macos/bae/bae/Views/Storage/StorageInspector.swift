import SwiftUI

/// The selected release's files and their live transfer activity.
struct StorageInspector: View {
    let releaseId: String?

    static func releaseId(in selection: Set<String>) -> String? {
        guard selection.count == 1 else { return nil }
        return selection.first
    }

    var body: some View {
        VStack(spacing: 0) {
            if let releaseId {
                StorageContentsInspector(releaseId: releaseId)
            }
            else {
                ContentUnavailableView(
                    "Select a release",
                    systemImage: "sidebar.trailing"
                )
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .frame(
            minWidth: 360,
            idealWidth: 440,
            maxWidth: 520,
            maxHeight: .infinity,
            alignment: .top
        )
    }
}
