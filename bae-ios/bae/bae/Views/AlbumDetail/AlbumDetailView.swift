import BaeKit
import SwiftUI

/// Album detail: a header, a release picker when the album has more than one
/// release, and the selected release's track list grouped by side. Tapping a
/// track plays the release starting there. Reads the shared `LibraryStore`
/// (`albumSummaries` + `releaseDetails`), loading any missing release detail
/// on demand. The store stays live, so sync updates re-render in place.
struct AlbumDetailView: View {
    let albumId: String
    private let context: AlbumDetailContext?

    @Environment(LibraryStore.self)
    private var libraryStore
    @Environment(Library.self)
    private var library
    @Environment(MediaPaths.self)
    private var mediaPaths
    @Environment(Playback.self)
    private var playback
    @Environment(Queue.self)
    private var queue

    @State
    private var selectedReleaseId: String?
    @State
    private var showGallery = false

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
                    detailPlaceholder(releaseId: selectedReleaseId)
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
                    detailPlaceholder(releaseId: releaseId)
                }
            }
            else {
                detailPlaceholder(releaseId: nil)
            }
        }
        .background(Theme.background)
        .navigationTitle("bae")
        .navigationBarTitleDisplayMode(.inline)
        .safeAreaInset(edge: .bottom) {
            NowPlayingBar()
        }
        .task(id: albumId) {
            if context != nil {
                if let releaseId = selectedReleaseId {
                    await libraryStore.loadReleaseDetail(
                        releaseId: releaseId,
                        library: library
                    )
                }
                return
            }
            // Eagerly load detail for every release so the picker has labels
            // and switching releases doesn't flash a spinner. N is typically
            // 1-3. Bail on cancellation (the user navigated away).
            guard let summary = libraryStore.albumSummaries[albumId] else {
                if let releaseId = selectedReleaseId {
                    await libraryStore.loadReleaseDetail(
                        releaseId: releaseId,
                        library: library
                    )
                }
                return
            }
            for releaseId in summary.releaseIds {
                if Task.isCancelled {
                    return
                }
                await libraryStore.loadReleaseDetail(
                    releaseId: releaseId,
                    library: library
                )
            }
        }
    }

    /// The pre-content placeholder for a release whose detail hasn't loaded:
    /// an error + Retry once its load has failed, otherwise a spinner. Retry
    /// re-runs the on-demand load for that release.
    @ViewBuilder
    private func detailPlaceholder(releaseId: String?) -> some View {
        if let releaseId, let error = libraryStore.releaseDetailErrors[releaseId] {
            LoadFailureView(line: error.line) {
                Task {
                    await libraryStore.loadReleaseDetail(
                        releaseId: releaseId,
                        library: library
                    )
                }
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
            VStack(alignment: .leading, spacing: 12) {
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
            .padding(16)
        }
        .fullScreenCover(isPresented: $showGallery) {
            GalleryView(
                items: detail.galleryItems,
                loadImage: { item in
                    // The lightbox passes the whole source to the bridge, which
                    // dispatches the read in core. The UI never inspects it.
                    try await mediaPaths.fetchGalleryBytes(
                        releaseId,
                        item.source
                    )
                }
            )
        }
    }

    private func releasePicker(summary: AlbumSummary) -> some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 8) {
                ForEach(summary.releaseIds, id: \.self) { id in
                    Button {
                        selectedReleaseId = id
                        Task {
                            await libraryStore.loadReleaseDetail(
                                releaseId: id,
                                library: library
                            )
                        }
                    } label: {
                        Group {
                            if let label = libraryStore.releaseDetails[id]?.displayName {
                                Text(label)
                                    .font(.callout)
                                    .lineLimit(1)
                            }
                            else {
                                ProgressView()
                                    .controlSize(.small)
                            }
                        }
                        .padding(.horizontal, 12)
                        .padding(.vertical, 6)
                        .background(
                            RoundedRectangle(cornerRadius: 16)
                                .fill(
                                    id == activeReleaseId(summary: summary)
                                        ? Theme.accentSoft
                                        : Theme.surfaceElevated
                                )
                        )
                    }
                    .buttonStyle(.plain)
                }
            }
        }
    }

    /// The active release: explicit selection, otherwise the album's primary release.
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

    /// The artist to show on `track`'s row, or `nil` for none. On the album
    /// screen this is core's decision (`displayArtist`, set only for a
    /// compilation); a work-release view shows the performer regardless, because
    /// its header is the work/composer — navigation context, decided here, not by
    /// core, since the same track also serves the album screen.
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
    NavigationStack {
        AlbumDetailView(albumId: "a-1")
    }
    .previewStores()
}
#endif
