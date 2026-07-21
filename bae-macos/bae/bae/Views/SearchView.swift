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
                    resultsList(results)
                }
            }
        }
        .frame(width: 440, height: 350)
        .background(
            RoundedRectangle(cornerRadius: 14)
                .fill(
                    LinearGradient(
                        colors: [Theme.surfaceElevated, Theme.surface],
                        startPoint: .top,
                        endPoint: .bottom
                    )
                )
        )
        .clipShape(RoundedRectangle(cornerRadius: 14))
        .overlay(
            RoundedRectangle(cornerRadius: 14)
                .strokeBorder(Color.white.opacity(0.08), lineWidth: 1)
        )
        .shadow(color: .black.opacity(0.5), radius: 24, y: 12)
    }

    private func resultsList(_ results: SearchResults) -> some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 2) {
                if !results.albums.isEmpty {
                    sectionHeader("Albums")
                    ForEach(results.albums, id: \.id) { album in
                        SearchResultRow(
                            leading: albumArt(album),
                            title: album.title,
                            subtitle: albumSubtitle(album),
                            action: { onSelectAlbum(album.id) }
                        )
                    }
                }

                if !results.tracks.isEmpty {
                    sectionHeader("Tracks")
                    ForEach(results.tracks, id: \.id) { track in
                        SearchResultRow(
                            leading: rowGlyph("waveform"),
                            title: track.title,
                            subtitle: trackSubtitle(track),
                            trailing: track.durationLabel.isEmpty
                                ? nil : track.durationLabel,
                            action: { onSelectAlbum(track.albumId) }
                        )
                    }
                }

                if !results.composers.isEmpty {
                    sectionHeader("Composers")
                    ForEach(results.composers, id: \.id) { composer in
                        SearchResultRow(
                            leading: rowGlyph("person.wave.2"),
                            title: composer.name,
                            subtitle:
                                "\(composer.workCount) \(String(localized: "Works"))",
                            action: { onSelectComposer(composer.id) }
                        )
                    }
                }

                if !results.works.isEmpty {
                    sectionHeader("Works")
                    ForEach(results.works, id: \.id) { work in
                        SearchResultRow(
                            leading: rowGlyph("music.quarternote.3"),
                            title: work.title,
                            subtitle: work.composerNames,
                            action: { onSelectWork(work.id) }
                        )
                    }
                }
            }
            .padding(8)
        }
    }

    private func sectionHeader(_ title: LocalizedStringKey) -> some View {
        Text(title)
            .font(.system(size: 12, weight: .heavy))
            .tracking(0.5)
            .foregroundStyle(.secondary)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, 8)
            .padding(.top, 8)
            .padding(.bottom, 2)
    }

    private func albumArt(_ album: AlbumSearchResult) -> some View {
        ImageView(imageRef: album.cover, pointSize: 46)
            .clipShape(RoundedRectangle(cornerRadius: 8))
    }

    private func rowGlyph(_ systemName: String) -> some View {
        Image(systemName: systemName)
            .font(.system(size: 18))
            .foregroundStyle(.secondary)
    }

    private func albumSubtitle(_ album: AlbumSearchResult) -> String {
        if let year = album.year {
            "\(album.artistName) (\(year))"
        }
        else {
            album.artistName
        }
    }

    private func trackSubtitle(_ track: TrackSearchResult) -> String {
        String(
            format: String(localized: "%@ - %@"),
            track.artistName,
            track.albumTitle
        )
    }
}

/// One search hit: leading art or glyph, a title over an optional subtitle, an
/// optional trailing label (a track's duration), with a subtle hover fill.
private struct SearchResultRow<Leading: View>: View {
    let leading: Leading
    let title: String
    let subtitle: String?
    var trailing: String?
    let action: () -> Void

    @State
    private var hovering = false

    var body: some View {
        Button(action: action) {
            HStack(spacing: 12) {
                leading
                    .frame(width: 46, height: 46)

                VStack(alignment: .leading, spacing: 2) {
                    Text(title)
                        .font(.system(size: 16, weight: .semibold))
                        .lineLimit(1)
                    StableOptionalText(
                        text: subtitle,
                        font: .system(size: 13, weight: .medium),
                        foreground: .secondary,
                        lineHeight: 14,
                        lineLimit: 1
                    )
                }

                Spacer(minLength: 8)

                if let trailing {
                    Text(trailing)
                        .font(.system(size: 14).monospacedDigit())
                        .foregroundStyle(.secondary)
                }
            }
            .padding(.horizontal, 8)
            .padding(.vertical, 6)
            .frame(maxWidth: .infinity, alignment: .leading)
            .contentShape(Rectangle())
            .background(
                RoundedRectangle(cornerRadius: 10)
                    .fill(Color.white.opacity(hovering ? 0.06 : 0))
            )
        }
        .buttonStyle(.plain)
        .onHover { hovering = $0 }
    }
}

#if DEBUG
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
#endif
