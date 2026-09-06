import BaeKit
import SwiftUI

/// Search an album on either metadata provider without selecting a release.
struct ImportSourceSearchMenu: View {
    let onSearch: (BridgeMetadataSource) -> Void

    var body: some View {
        Menu {
            Button("Search \(bridgeMetadataSourceName(source: .musicBrainz))") {
                onSearch(.musicBrainz)
            }
            Button("Search \(bridgeMetadataSourceName(source: .discogs))") {
                onSearch(.discogs)
            }
        } label: {
            Label("Search", systemImage: "magnifyingglass")
        }
        .menuStyle(.borderlessButton)
        .fixedSize()
    }
}
