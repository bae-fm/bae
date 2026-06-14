import SwiftUI

/// Library search results: two sections — Albums then Tracks. Both row types
/// open the album's detail; a track navigates to its album rather than playing.
/// Iterates and renders only — the search call and the bridge→model mapping
/// live in the data layer (`Library.searchLibrary` / `SearchResults`).
struct SearchResultsView: View {
    let results: SearchResults?
    let error: String?
    let onSelect: (String) -> Void

    @Environment(MediaPaths.self)
    private var mediaPaths

    var body: some View {
        if let error {
            centered(Text(error).foregroundStyle(.red))
        }
        else if let results {
            if results.albums.isEmpty, results.tracks.isEmpty {
                centered(
                    Text("No results for \u{201C}\(results.query)\u{201D}")
                        .foregroundStyle(.secondary)
                )
            }
            else {
                List {
                    if !results.albums.isEmpty {
                        Section("Albums") {
                            ForEach(results.albums) { album in
                                Button { onSelect(album.id) } label: {
                                    AlbumResultRow(
                                        album: album,
                                        coverPath: mediaPaths.imagePathIfExists(
                                            album.primaryReleaseId
                                        )
                                    )
                                }
                                .buttonStyle(.plain)
                            }
                        }
                    }
                    if !results.tracks.isEmpty {
                        Section("Tracks") {
                            ForEach(results.tracks) { track in
                                Button { onSelect(track.albumId) } label: {
                                    TrackResultRow(track: track)
                                }
                                .buttonStyle(.plain)
                            }
                        }
                    }
                }
                .listStyle(.plain)
            }
        }
        else {
            centered(ProgressView())
        }
    }

    private func centered(_ view: some View) -> some View {
        view
            .multilineTextAlignment(.center)
            .padding(32)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

private struct AlbumResultRow: View {
    let album: AlbumSearchResult
    let coverPath: String?

    var body: some View {
        HStack(spacing: 12) {
            ImageView(path: coverPath, pointSize: 48)
                .frame(width: 48, height: 48)
                .clipShape(RoundedRectangle(cornerRadius: 4))
            VStack(alignment: .leading, spacing: 2) {
                Text(album.title)
                    .font(.body)
                    .lineLimit(1)
                Text(album.year.map { "\(album.artistName) \u{00B7} \($0)" } ?? album.artistName)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            Spacer()
        }
    }
}

private struct TrackResultRow: View {
    let track: TrackSearchResult

    var body: some View {
        HStack(spacing: 12) {
            VStack(alignment: .leading, spacing: 2) {
                Text(track.title)
                    .font(.body)
                    .lineLimit(1)
                Text("\(track.artistName) \u{2014} \(track.albumTitle)")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            Spacer()
            if !track.durationLabel.isEmpty {
                Text(track.durationLabel)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
    }
}
