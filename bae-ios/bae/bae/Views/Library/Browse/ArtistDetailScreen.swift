import BaeKit
import SwiftUI

/// An artist's albums, loaded on demand by artist id.
struct ArtistDetailScreen: View {
    let artistId: String
    let openAlbum: (String) -> Void

    @Environment(LibraryProjectionStore.self)
    private var libraryProjections

    private var detail: BridgeArtistDetail? { libraryProjections.artist.value }
    private var error: String? {
        libraryProjections.artist.error?.line
            ?? (libraryProjections.artist.delivered && detail == nil
                ? String(localized: "Artist detail not found") : nil)
    }

    var body: some View {
        Group {
            if let detail {
                ArtistDetailContent(detail: detail, openAlbum: openAlbum)
                    .overlay(alignment: .top) {
                        if let error {
                            Text(error).foregroundStyle(.red).padding(12)
                        }
                    }
            }
            else if let error {
                Text(error).foregroundStyle(.red).padding(32)
            }
            else {
                ProgressView()
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .navigationTitle(navigationTitle)
        .navigationBarTitleDisplayMode(.inline)
        .onAppear { libraryProjections.activateArtist(artistId) }
        .onDisappear { libraryProjections.deactivateArtist(artistId) }
    }

    private var navigationTitle: String {
        if let detail {
            return detail.artist.name
        }
        if error != nil {
            return String(localized: "Artists")
        }
        return ""
    }

}

private struct ArtistDetailContent: View {
    let detail: BridgeArtistDetail
    let openAlbum: (String) -> Void

    var body: some View {
        List {
            Section {
                ArtistSummaryRow(summary: detail.artist)
            }
            if !detail.albums.isEmpty {
                Section("Albums") {
                    ForEach(detail.albums) { album in
                        Button {
                            openAlbum(album.id)
                        } label: {
                            HStack(spacing: 12) {
                                ImageView(imageRef: album.cover, pointSize: 48)
                                    .frame(width: 48, height: 48)
                                    .clipShape(RoundedRectangle(cornerRadius: 6))
                                TwoLineRow(
                                    title: album.title,
                                    subtitle: album.year.map(String.init)
                                )
                            }
                        }
                    }
                }
            }
        }
        .listStyle(.insetGrouped)
    }
}

#if DEBUG
#Preview {
    NavigationStack {
        ArtistDetailScreen(artistId: "artist-1", openAlbum: { _ in })
    }
    .previewStores()
}
#endif
