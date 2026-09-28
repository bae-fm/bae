import BaeKit
import SwiftUI

/// Library search results; album and track rows open album detail, and the
/// other rows open their own screens.
struct SearchResultsView: View {
    let results: SearchResults?
    let error: DisplayError?
    let onSelectAlbum: (String) -> Void
    let onSelectArtist: (String) -> Void
    let onSelectComposer: (String) -> Void
    let onSelectWork: (String) -> Void

    var body: some View {
        if let error {
            centered(ErrorDetailDisclosure(error: error))
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
                                    ArtistSummaryRow(summary: artist)
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
                                    ComposerSummaryRow(summary: composer)
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
            .padding(ThemeSpace.page)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

private struct AlbumResultRow: View {
    let album: AlbumSearchResult

    var body: some View {
        HStack(spacing: ThemeSpace.group) {
            ImageView(imageRef: album.cover, pointSize: ThemeSize.rowArtwork)
                .frame(width: ThemeSize.rowArtwork, height: ThemeSize.rowArtwork)
                .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.artwork))
            TwoLineRow(
                title: album.title,
                subtitle: album.year.map { "\(album.artistName) \u{00B7} \($0)" }
                    ?? album.artistName
            )
            Spacer()
        }
    }
}

private struct TrackResultRow: View {
    let track: TrackSearchResult

    var body: some View {
        HStack(spacing: ThemeSpace.group) {
            TwoLineRow(
                title: track.title,
                subtitle: String(
                    localized: "\(track.artistName), \(track.albumTitle)"
                )
            )
            Spacer()
            if !track.durationLabel.isEmpty {
                Text(track.durationLabel)
                    .themeText(.detail)
                    .foregroundStyle(.secondary)
            }
        }
    }
}

private struct WorkResultRow: View {
    let work: BridgeWorkSummary

    var body: some View {
        HStack(spacing: ThemeSpace.group) {
            ImageView(imageRef: work.representativeCover, pointSize: ThemeSize.rowArtwork)
                .frame(width: ThemeSize.rowArtwork, height: ThemeSize.rowArtwork)
                .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.artwork))
            TwoLineRow(title: work.title, subtitle: work.composerNames)
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
