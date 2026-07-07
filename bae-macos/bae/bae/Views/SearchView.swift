import BaeKit
import SwiftUI

struct SearchView: View {
    let results: SearchResults?
    let onSelectAlbum: (String) -> Void
    let onSelectComposer: (String) -> Void
    let onSelectWork: (String) -> Void

    var body: some View {
        Group {
            if let results {
                if results.albums.isEmpty, results.tracks.isEmpty,
                    results.composers.isEmpty, results.works.isEmpty
                {
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

            Section("Composers") {
                ForEach(results.composers, id: \.id) { composer in
                    composerRow(composer)
                }
            }
            .opacity(results.composers.isEmpty ? 0 : 1)
            .allowsHitTesting(!results.composers.isEmpty)

            Section("Works") {
                ForEach(results.works, id: \.id) { work in
                    workRow(work)
                }
            }
            .opacity(results.works.isEmpty ? 0 : 1)
            .allowsHitTesting(!results.works.isEmpty)
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

                    Text(
                        String(
                            format: String(localized: "%@ - %@"),
                            track.artistName,
                            track.albumTitle
                        )
                    )
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

    private func composerRow(_ composer: BridgeComposerSummary) -> some View {
        libraryEntityRow(
            systemImage: "person.wave.2",
            title: composer.name,
            subtitle: "\(composer.workCount) \(String(localized: "Works"))",
            action: { onSelectComposer(composer.id) }
        )
    }

    private func workRow(_ work: BridgeWorkSummary) -> some View {
        libraryEntityRow(
            systemImage: "music.quarternote.3",
            title: work.title,
            subtitle: work.composerNames,
            action: { onSelectWork(work.id) }
        )
    }

    private func libraryEntityRow(
        systemImage: String,
        title: String,
        subtitle: String?,
        action: @escaping () -> Void
    ) -> some View {
        Button(action: action) {
            HStack(spacing: 12) {
                Image(systemName: systemImage)
                    .frame(width: 32, height: 32)
                    .foregroundStyle(.secondary)

                VStack(alignment: .leading, spacing: 2) {
                    Text(title)
                        .font(.body)
                        .lineLimit(1)
                    StableOptionalText(
                        text: subtitle,
                        font: .caption,
                        foreground: .secondary,
                        lineHeight: 12,
                        lineLimit: 1
                    )
                }
            }
        }
        .buttonStyle(.plain)
    }

    private func albumArt(_ album: AlbumSearchResult) -> some View {
        ImageView(imageRef: album.cover, pointSize: 32)
    }
}

// MARK: - Previews

#Preview("With results") {
    SearchView(
        results: PreviewData.searchResults,
        onSelectAlbum: { _ in },
        onSelectComposer: { _ in },
        onSelectWork: { _ in },
    )
    .frame(width: 600, height: 500)
    .environment(MediaPaths.stub)
}

#Preview("No results") {
    SearchView(
        results: SearchResults(
            bridge: BridgeSearchResults(
                albums: [],
                tracks: [],
                composers: [],
                works: []
            ),
            query: "placeholder"
        ),
        onSelectAlbum: { _ in },
        onSelectComposer: { _ in },
        onSelectWork: { _ in },
    )
    .frame(width: 600, height: 400)
    .environment(MediaPaths.stub)
}
