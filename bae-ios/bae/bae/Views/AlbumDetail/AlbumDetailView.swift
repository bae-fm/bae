import BaeKit
import SwiftUI

/// Album detail: a header, a release picker when the album has several
/// releases, and the selected release's tracks grouped by side.
struct AlbumDetailView: View {
    let albumId: String
    private let context: AlbumDetailContext?

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
        context: AlbumDetailContext? = nil
    ) {
        self.albumId = albumId
        self.context = context
        _selectedReleaseId = State(initialValue: initialReleaseId)
    }

    var body: some View {
        Group {
            if let context {
                if let releaseId = selectedReleaseId,
                    let detail = libraryStore.releaseDetails[releaseId]
                {
                    content(
                        display: AlbumDetailDisplay(context: context),
                        releasePickerSummary: nil,
                        releaseId: releaseId,
                        detail: detail
                    )
                }
                else {
                    detailPlaceholder()
                }
            }
            else if let summary = libraryStore.albumSummaries[albumId] {
                let releaseId = activeReleaseId(summary: summary)
                if let detail = libraryStore.releaseDetails[releaseId] {
                    content(
                        display: AlbumDetailDisplay(summary: summary),
                        releasePickerSummary: summary,
                        releaseId: releaseId,
                        detail: detail
                    )
                }
                else {
                    detailPlaceholder()
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
            LoadFailureView(line: error.line) {
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
                    onPlayTrackAt: { index in
                        playback.playRelease(releaseId, UInt32(index), false)
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

enum AlbumDetailDisplay {
    case album(AlbumSummary)
    case workRelease(AlbumDetailContext)

    init(summary: AlbumSummary) {
        self = .album(summary)
    }

    init(context: AlbumDetailContext) {
        self = .workRelease(context)
    }

    var title: String {
        switch self {
        case .album(let summary):
            summary.title
        case .workRelease(let context):
            context.title
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

struct AlbumDetailContext: Hashable {
    let title: String

    init(workRelease: BridgeWorkReleaseSummary) {
        title = workRelease.albumTitle
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
