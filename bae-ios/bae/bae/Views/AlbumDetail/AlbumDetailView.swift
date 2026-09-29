import BaeKit
import SwiftUI

/// Album detail: a header, a release picker when the album has several
/// releases, and the selected release's tracks grouped by side.
struct AlbumDetailView: View {
    let albumId: String
    private let entry: AlbumDetailEntry

    @Environment(LibraryStore.self)
    private var libraryStore
    @Environment(Library.self)
    private var library
    @Environment(ImageStore.self)
    private var imageStore
    @Environment(Playback.self)
    private var playback
    @Environment(Queue.self)
    private var queue

    @State
    private var selectedReleaseId: String?
    @State
    private var showGallery = false
    /// The view's one album read, moved as `albumId` changes.
    @State
    private var detailReader: DetailReader<BridgeAlbumDetail>?

    init(
        albumId: String,
        initialReleaseId: String? = nil,
        entry: AlbumDetailEntry = .album
    ) {
        self.albumId = albumId
        self.entry = entry
        _selectedReleaseId = State(initialValue: initialReleaseId)
    }

    var body: some View {
        Group {
            if let summary = libraryStore.albumSummaries[albumId] {
                switch entry {
                case .album:
                    let releaseId = activeReleaseId(summary: summary)
                    if let detail = libraryStore.releaseDetails[releaseId] {
                        content(
                            display: .album(summary),
                            releasePickerSummary: summary,
                            releaseId: releaseId,
                            detail: detail
                        )
                    }
                    else {
                        detailPlaceholder()
                    }
                case .workRelease:
                    if let releaseId = selectedReleaseId,
                        let detail = libraryStore.releaseDetails[releaseId]
                    {
                        content(
                            display: .workRelease(summary),
                            releasePickerSummary: nil,
                            releaseId: releaseId,
                            detail: detail
                        )
                    }
                    else {
                        detailPlaceholder()
                    }
                }
            }
            else {
                detailPlaceholder()
            }
        }
        .background(Theme.background)
        .navigationTitle("bae")
        .navigationBarTitleDisplayMode(.inline)
        .safeAreaInset(edge: .bottom) {
            NowPlayingBar()
        }
        .onAppear { showAlbum(albumId) }
        .onChange(of: albumId) { _, newId in showAlbum(newId) }
        .onDisappear { detailReader?.close() }
    }

    private func showAlbum(_ albumId: String) {
        let reader =
            detailReader ?? libraryStore.albumDetailReader(library: library)
        detailReader = reader
        reader.show(albumId)
    }

    /// The album's load error with Retry once loading has failed, otherwise a
    /// spinner.
    @ViewBuilder
    private func detailPlaceholder() -> some View {
        if let error = libraryStore.albumDetailErrors[albumId] {
            LoadFailureView(error: error) {
                detailReader?.retry()
            }
        }
        else {
            ProgressView()
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
    }

    private func content(
        display: AlbumDetailDisplay,
        releasePickerSummary: AlbumSummary?,
        releaseId: String,
        detail: ReleaseDetail
    ) -> some View {
        ScrollView {
            VStack(alignment: .leading, spacing: ThemeSpace.group) {
                AlbumDetailHeader(
                    display: display,
                    releaseId: releaseId,
                    detail: detail,
                    showGallery: $showGallery
                )
                ReleaseDownloadSection(releaseId: releaseId, detail: detail)
                if let summary = releasePickerSummary,
                    summary.releaseIds.count > 1
                {
                    releasePicker(summary: summary)
                }
                TrackList(
                    detail: detail,
                    artistDisplay: display.trackArtistDisplay,
                    onPlayTrack: { trackId in
                        playback.playRelease(releaseId, trackId, false)
                    },
                    onPlayNext: { trackId in queue.addNext([trackId]) },
                    onAddToQueue: { trackId in queue.addToQueue([trackId]) }
                )
            }
            .padding(ThemeSpace.edge)
        }
        .fullScreenCover(isPresented: $showGallery) {
            GalleryView(
                items: detail.galleryItems,
                loadImage: { item in
                    // Core, not this view, decides how to read each source.
                    try await imageStore.decodeSource(
                        for: .releaseImage(
                            releaseId: releaseId,
                            source: item.source
                        ),
                        at: .nativeResolution
                    )
                }
            )
        }
    }

    private func releasePicker(summary: AlbumSummary) -> some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: ThemeSpace.related) {
                ForEach(summary.releaseIds, id: \.self) { id in
                    let tone: StatusTone =
                        id == activeReleaseId(summary: summary)
                        ? .accent : .neutral
                    Button {
                        selectedReleaseId = id
                    } label: {
                        Group {
                            if let label = libraryStore.releaseDetails[id]?.displayName {
                                Text(label)
                                    .themeText(.body)
                                    .lineLimit(1)
                            }
                            else {
                                ProgressView()
                                    .controlSize(.small)
                            }
                        }
                        .foregroundStyle(tone.color)
                        .padding(.horizontal, ThemeSpace.compact)
                        .padding(.vertical, ThemeSpace.line)
                        .background(
                            tone.fill,
                            in: RoundedRectangle(cornerRadius: ThemeRadius.chip)
                        )
                    }
                    .buttonStyle(.plain)
                }
            }
        }
    }

    /// The selected release when it belongs to the album, else the primary one.
    private func activeReleaseId(summary: AlbumSummary) -> String {
        if let id = selectedReleaseId, summary.releaseIds.contains(id) {
            return id
        }
        precondition(
            summary.releaseIds.contains(summary.primaryReleaseId),
            "primaryReleaseId missing from releaseIds for album \(summary.id)"
        )
        return summary.primaryReleaseId
    }
}

/// How the screen was reached: from the album list, or from a work as one
/// release of it. The route carries only this and ids; the header reads the
/// album's live summary either way.
enum AlbumDetailEntry: Hashable {
    case album
    case workRelease
}

enum AlbumDetailDisplay {
    case album(AlbumSummary)
    case workRelease(AlbumSummary)

    var title: String {
        switch self {
        case .album(let summary), .workRelease(let summary):
            summary.title
        }
    }

    var albumMetadata: AlbumDetailAlbumMetadata? {
        switch self {
        case .album(let summary):
            AlbumDetailAlbumMetadata(
                artistNames: summary.artistNames,
                year: summary.year
            )
        case .workRelease:
            nil
        }
    }

    var trackArtistDisplay: TrackArtistDisplay {
        switch self {
        case .album:
            .album
        case .workRelease:
            .workRelease
        }
    }
}

struct AlbumDetailAlbumMetadata {
    let artistNames: String
    let year: Int32?
}

enum TrackArtistDisplay {
    case album
    case workRelease

    /// The artist to show on `track`'s row, or `nil` for none: core's choice on
    /// the album screen, always the performer on a work release, whose header
    /// names the work instead.
    func artist(for track: Track) -> String? {
        switch self {
        case .album:
            track.displayArtist
        case .workRelease:
            track.artistNames
        }
    }
}

#if DEBUG
#Preview {
    PreviewScenes.albumDetail()
}
#endif
