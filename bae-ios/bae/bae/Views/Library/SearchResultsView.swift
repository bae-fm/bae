import BaeKit
import SwiftUI

/// Library search results. Album and track rows open album detail; artist,
/// composer, and work rows navigate to their bridge ids.
/// Iterates and renders only — the search call and the bridge→model mapping
/// live in the data layer (`Library.searchLibrary` / `SearchResults`).
struct SearchResultsView: View {
    let results: SearchResults?
    let error: String?
    let onSelectAlbum: (String) -> Void
    let onSelectArtist: (String) -> Void
    let onSelectComposer: (String) -> Void
    let onSelectWork: (String) -> Void

    var body: some View {
        if let error {
            centered(Text(error).foregroundStyle(.red))
        }
        else if let results {
            if results.albums.isEmpty, results.artists.isEmpty,
                results.tracks.isEmpty, results.composers.isEmpty,
                results.works.isEmpty
            {
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
                                Button {
                                    onSelectAlbum(album.id)
                                } label: {
                                    AlbumResultRow(album: album)
                                }
                                .buttonStyle(.plain)
                            }
                        }
                    }
                    if !results.artists.isEmpty {
                        Section("Artists") {
                            ForEach(results.artists, id: \.artistId) { artist in
                                Button {
                                    onSelectArtist(artist.artistId)
                                } label: {
                                    ArtistResultRow(artist: artist)
                                }
                                .buttonStyle(.plain)
                            }
                        }
                    }
                    if !results.tracks.isEmpty {
                        Section("Tracks") {
                            ForEach(results.tracks) { track in
                                Button {
                                    onSelectAlbum(track.albumId)
                                } label: {
                                    TrackResultRow(track: track)
                                }
                                .buttonStyle(.plain)
                            }
                        }
                    }
                    if !results.composers.isEmpty {
                        Section("Composers") {
                            ForEach(results.composers, id: \.artistId) {
                                composer in
                                Button {
                                    onSelectComposer(composer.artistId)
                                } label: {
                                    ComposerResultRow(composer: composer)
                                }
                                .buttonStyle(.plain)
                            }
                        }
                    }
                    if !results.works.isEmpty {
                        Section("Works") {
                            ForEach(results.works, id: \.workId) { work in
                                Button {
                                    onSelectWork(work.workId)
                                } label: {
                                    WorkResultRow(work: work)
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

    var body: some View {
        HStack(spacing: 12) {
            ImageView(imageRef: album.cover, pointSize: 48)
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

private struct ArtistResultRow: View {
    let artist: BridgeArtistSummary

    var body: some View {
        HStack(spacing: 12) {
            ImageView(imageRef: artist.image, pointSize: 48)
                .frame(width: 48, height: 48)
                .clipShape(RoundedRectangle(cornerRadius: 4))
            VStack(alignment: .leading, spacing: 2) {
                Text(artist.name)
                    .font(.body)
                    .lineLimit(1)
                Text("\(artist.albumCount) albums")
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

private struct ComposerResultRow: View {
    let composer: BridgeComposerSummary

    var body: some View {
        HStack(spacing: 12) {
            ImageView(imageRef: composer.image, pointSize: 48)
                .frame(width: 48, height: 48)
                .clipShape(RoundedRectangle(cornerRadius: 4))
            VStack(alignment: .leading, spacing: 2) {
                Text(composer.name)
                    .font(.body)
                    .lineLimit(1)
                Text("\(composer.workCount) works")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            Spacer()
        }
    }
}

private struct WorkResultRow: View {
    let work: BridgeWorkSummary

    var body: some View {
        HStack(spacing: 12) {
            ImageView(imageRef: work.representativeCover, pointSize: 48)
                .frame(width: 48, height: 48)
                .clipShape(RoundedRectangle(cornerRadius: 4))
            VStack(alignment: .leading, spacing: 2) {
                Text(work.title)
                    .font(.body)
                    .lineLimit(1)
                if let composers = work.composerNames {
                    Text(composers)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
            }
            Spacer()
        }
    }
}

#if DEBUG
#Preview {
    SearchResultsView(
        results: PreviewData.searchResults,
        error: nil,
        onSelectAlbum: { _ in },
        onSelectArtist: { _ in },
        onSelectComposer: { _ in },
        onSelectWork: { _ in }
    )
    .previewStores()
}
#endif
