import BaeKit
import SwiftUI

/// The selected release's files and their live transfer activity.
struct StorageInspector: View {
    /// The file whose details open with the first release shown.
    let initialSelectedFileId: String?

    @Environment(StorageManagerStore.self)
    private var storageManagerStore

    var body: some View {
        VStack(spacing: 0) {
            // The inspector shows a release only when the list holds exactly
            // one of the selected releases, so it never shows a release the
            // list does not show.
            if let releaseId = storageManagerStore.soleRelease {
                StorageContentsInspector(
                    releaseId: releaseId,
                    initialSelectedFileId: initialSelectedFileId
                )
            }
            else {
                MacContentUnavailable(title: "Select a release")
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
