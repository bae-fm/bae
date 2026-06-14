import SwiftUI

struct SearchView: View {
    let results: SearchResults?
    let resolveImagePath: (String?) -> String?
    let onSelectAlbum: (String) -> Void

    var body: some View {
        Group {
            if let results {
                if results.albums.isEmpty, results.tracks.isEmpty {
                    ContentUnavailableView.search(text: results.query)
                }
                else {
                    searchResultsList(results)
                }
            }
        }
        .frame(width: 400, height: 350)
        .background(Theme.surface)
        .clipShape(RoundedRectangle(cornerRadius: 8))
        .shadow(radius: 8)
    }

    private func searchResultsList(_ results: SearchResults) -> some View {
        List {
            if !results.albums.isEmpty {
                Section("Albums") {
                    ForEach(results.albums, id: \.id) { album in
                        albumRow(album)
                    }
                }
            }

            if !results.tracks.isEmpty {
                Section("Tracks") {
                    ForEach(results.tracks, id: \.id) { track in
                        trackRow(track)
                    }
                }
            }
        }
        .scrollContentBackground(.hidden)
        .background(Theme.background)
    }

    private func albumRow(_ album: AlbumSearchResult) -> some View {
        Button(action: { onSelectAlbum(album.id) }) {
            HStack(spacing: 12) {
                albumArt(album)
                    .frame(width: 32, height: 32)
                    .clipShape(RoundedRectangle(cornerRadius: 4))

                VStack(alignment: .leading, spacing: 2) {
                    Text(album.title)
                        .font(.body)
                        .lineLimit(1)

                    HStack(spacing: 4) {
                        Text(album.artistName)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                            .lineLimit(1)

                        if let year = album.year {
                            Text("(\(String(year)))")
                                .font(.caption)
                                .foregroundStyle(.tertiary)
                        }
                    }
                }
            }
        }
        .buttonStyle(.plain)
    }

    private func trackRow(_ track: TrackSearchResult) -> some View {
        Button(action: {
            onSelectAlbum(track.albumId)
        }) {
            HStack(spacing: 12) {
                Image(systemName: "waveform")
                    .frame(width: 32, height: 32)
                    .foregroundStyle(.secondary)

                VStack(alignment: .leading, spacing: 2) {
                    Text(track.title)
                        .font(.body)
                        .lineLimit(1)

                    Text("\(track.artistName) - \(track.albumTitle)")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }

                Spacer()

                if !track.durationLabel.isEmpty {
                    Text(track.durationLabel)
                        .font(.callout.monospacedDigit())
                        .foregroundStyle(.secondary)
                }
            }
        }
        .buttonStyle(.plain)
    }

    private func albumArt(_ album: AlbumSearchResult) -> some View {
        ImageView(
            localPath: resolveImagePath(album.primaryReleaseId),
            pointSize: 32
        )
    }
}

// MARK: - Previews

#Preview("With results") {
    SearchView(
        results: SearchResults(
            bridge: BridgeSearchResults(
                albums: [
                    BridgeAlbumSearchResult(
                        id: "a-02",
                        title: "Pacific Standard",
                        year: 2019,
                        primaryReleaseId: "r-02",
                        artistName: "Glass Harbor"
                    ),
                    BridgeAlbumSearchResult(
                        id: "a-14",
                        title: "Landlocked",
                        year: 2022,
                        primaryReleaseId: "r-14",
                        artistName: "Glass Harbor"
                    ),
                    BridgeAlbumSearchResult(
                        id: "a-03",
                        title: "Proof by Induction",
                        year: 2021,
                        primaryReleaseId: "r-03",
                        artistName: "Velvet Mathematics"
                    ),
                ],
                tracks: [
                    BridgeTrackSearchResult(
                        id: "t-03",
                        title: "Tide Pool",
                        durationMs: 198_000,
                        durationLabel: "3:18",
                        albumId: "a-02",
                        albumTitle: "Pacific Standard",
                        artistName: "Glass Harbor"
                    ),
                    BridgeTrackSearchResult(
                        id: "t-05",
                        title: "Axiom",
                        durationMs: 187_000,
                        durationLabel: "3:07",
                        albumId: "a-03",
                        albumTitle: "Proof by Induction",
                        artistName: "Velvet Mathematics"
                    ),
                ],
            ),
            query: "glass"
        ),
        resolveImagePath: { _ in nil },
        onSelectAlbum: { _ in },
    )
    .frame(width: 600, height: 500)
    .environment(MediaPaths.stub)
}

#Preview("No results") {
    SearchView(
        results: SearchResults(
            bridge: BridgeSearchResults(albums: [], tracks: []),
            query: "nonexistent"
        ),
        resolveImagePath: { _ in nil },
        onSelectAlbum: { _ in },
    )
    .frame(width: 600, height: 400)
    .environment(MediaPaths.stub)
}
