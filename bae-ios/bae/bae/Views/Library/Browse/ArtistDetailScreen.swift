import BaeKit
import SwiftUI

/// An artist's albums, loaded on demand by artist id.
struct ArtistDetailScreen: View {
    let artistId: String
    let openAlbum: (String) -> Void

    @Environment(Library.self)
    private var library

    @State
    private var detail: BridgeArtistDetail?
    @State
    private var error: String?

    var body: some View {
        Group {
            if let error {
                Text(error)
                    .foregroundStyle(.red)
                    .padding(32)
            }
            else if let detail {
                ArtistDetailContent(detail: detail, openAlbum: openAlbum)
            }
            else {
                ProgressView()
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .navigationTitle(navigationTitle)
        .navigationBarTitleDisplayMode(.inline)
        .task(id: artistId) {
            await load()
        }
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

    private func load() async {
        error = nil
        do {
            let getArtistDetail = library.getArtistDetail
            let loaded =
                try await Task.detached {
                    try await getArtistDetail(artistId)
                }
                .value
            try Task.checkCancellation()
            guard let loaded else {
                error = String(localized: "Artist detail not found")
                return
            }
            detail = loaded
        }
        catch is CancellationError {}
        catch {
            self.error = error.displayLine
        }
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
