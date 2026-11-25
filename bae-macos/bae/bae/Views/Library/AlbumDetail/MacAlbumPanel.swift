import BaeKit
import SwiftUI

/// The expanded album-detail card; `AlbumDetailView` owns the state and
/// supplies every callback.
struct MacAlbumPanel: View {
    /// The album and its releases as its detail read returned them; the
    /// release picker names each release from here.
    let detail: AlbumDetail
    let onBrowseImages: () -> Void
    /// Show the release with this id, which the person picked in the release
    /// picker.
    let onSelectRelease: (String) -> Void
    let currentTrackId: String?
    /// The release of the track playing now, which the release picker marks.
    let currentReleaseId: String?
    let isPlaying: Bool
    let onClose: () -> Void
    let onPlay: () -> Void
    let onShuffle: () -> Void
    let onPlayFromTrack: (String) -> Void
    let onTogglePlayPause: () -> Void
    let onAddNext: (String) -> Void
    let onAddToQueue: (String) -> Void
    let onAddNextAlbum: () -> Void
    let onAddAlbumToQueue: () -> Void
    /// Open the release editor: cover, fields, album and catalog search.
    let onEdit: () -> Void
    let onOpenStorage: () -> Void
    let onExportRelease: () -> Void
    let onSaveReleaseAs: () -> Void
    let onSetPrimaryRelease: () -> Void
    let onDeleteRelease: () -> Void
    let onDeleteAlbum: () -> Void
    let onExportTrack: (String) -> Void
    /// Opens the work's page.
    let onOpenWork: (String) -> Void
    /// Rate the album with these stars, or clear its rating with none.
    let onRateAlbum: (BridgeStars?) -> Void
    /// Rate a track with these stars, or clear its rating with none.
    let onRateTrack: (String, BridgeStars?) -> Void
    /// Turn the person's heart on the album on or off, returning once the
    /// write does.
    let onSetAlbumHearted: (Bool) async -> Void
    /// Turn the person's heart on a track on or off, returning once the write
    /// does.
    let onSetTrackHearted: (String, Bool) async -> Void
    var playlists = TrackPlaylistChoices.none
    /// How the person picks several tracks in the list, or `nil` when the
    /// list does not allow picking.
    var picking: TrackPicking?
    /// Draw the help tag of Not on this device, which a shot cannot get from
    /// the system.
    var showsUnplayableHelp = false

    /// The heart the person chose for the album, until its write returns.
    @State
    private var pendingHeart = PendingHeart()

    /// The album cover's side.
    private static let coverSize: CGFloat = 340

    private var album: BridgeAlbum { detail.album }

    /// The release the page shows, as its detail read chose it.
    private var selectedRelease: ReleaseDetail { detail.shownRelease }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(alignment: .top, spacing: ThemeSpace.page) {
                albumArt
                    .frame(width: Self.coverSize, height: Self.coverSize)
                    .clipShape(
                        RoundedRectangle(cornerRadius: ThemeRadius.cover)
                    )
                    // The board's cover shadow is 0 12px 20px.
                    .background {
                        ShadowedFace(
                            cornerRadius: ThemeRadius.cover,
                            radius: 20,
                            offset: 12
                        )
                    }
                    .contentShape(Rectangle())
                    .onTapGesture(perform: onBrowseImages)
                VStack(alignment: .leading, spacing: ThemeSpace.inline) {
                    MacAlbumHeader(
                        detail: detail,
                        currentReleaseId: currentReleaseId,
                        onSelectRelease: onSelectRelease,
                        onPlay: onPlay,
                        rating: shownRating,
                        showsUnplayableHelp: showsUnplayableHelp
                    ) {
                        albumMenuItems
                    }
                    MacTrackColumns(
                        release: selectedRelease,
                        isCompilation: album.isCompilation,
                        currentTrackId: currentTrackId,
                        isPlaying: isPlaying,
                        onPlayFromTrack: onPlayFromTrack,
                        onTogglePlayPause: onTogglePlayPause,
                        onAddNext: onAddNext,
                        onAddToQueue: onAddToQueue,
                        onExportTrack: onExportTrack,
                        onRateTrack: onRateTrack,
                        onSetTrackHearted: onSetTrackHearted,
                        playlists: playlists,
                        picking: picking
                    )
                    .padding(.top, ThemeSpace.edge)
                    if selectedRelease.hasCredits {
                        MacAlbumCredits(credits: selectedRelease.credits)
                            .padding(.top, ThemeSpace.edge)
                    }
                    if selectedRelease.performsWorks {
                        MacAlbumWorks(
                            tracks: selectedRelease.tracks,
                            onOpenWork: onOpenWork
                        )
                    }
                    if let sourceAudio = selectedRelease.sourceAudio {
                        SourceAudioLine(sourceAudio: sourceAudio)
                            .padding(.top, ThemeSpace.related)
                    }
                }
            }
        }
        .padding(ThemeSpace.page)
        .background(
            Theme.surfaceElevated,
            in: RoundedRectangle(cornerRadius: ThemeRadius.panel)
        )
        .overlay(
            RoundedRectangle(cornerRadius: ThemeRadius.panel)
                .strokeBorder(Theme.hairline, lineWidth: 1)
        )
        // The board's panel shadow is 0 18px 28px.
        .background {
            ShadowedFace(
                cornerRadius: ThemeRadius.panel,
                radius: 28,
                offset: 18
            )
        }
        .overlay(alignment: .topTrailing) {
            MacCloseButton(onClose: onClose)
                .padding(ThemeSpace.edge)
        }
    }

    private var albumMenuItems: AlbumMenu {
        AlbumMenu(
            offersSetAsPrimary: detail.shownReleaseCanBecomePrimary,
            rating: shownRating,
            onRate: onRateAlbum,
            onSetHearted: { pendingHeart.choose($0, write: onSetAlbumHearted) },
            onShuffle: onShuffle,
            onAddNext: onAddNextAlbum,
            onAddToQueue: onAddAlbumToQueue,
            onEdit: onEdit,
            onOpenStorage: onOpenStorage,
            onExport: onExportRelease,
            onSaveAs: onSaveReleaseAs,
            onSetPrimaryRelease: onSetPrimaryRelease,
            onDeleteRelease: onDeleteRelease,
            onDeleteAlbum: onDeleteAlbum
        )
    }

    /// The album's rating, with the heart the person just chose in place of
    /// the one the read holds until the write returns.
    private var shownRating: BridgeAlbumRating {
        guard let choice = pendingHeart.choice,
            case .heart(_, let hearters) = detail.rating
        else { return detail.rating }
        return .heart(hearted: choice, hearters: hearters)
    }

    private var albumArt: some View {
        Artwork(
            imageRef: selectedRelease.summary.cover,
            pointSize: Self.coverSize,
            cornerRadius: nil
        )
    }
}

/// The release's edition line and copy line, each cut short on its own. When a
/// catalog describes the release, an arrow beside the first line marks them
/// and a click opens a popover of the catalogs' links.
struct ReleaseFactsLine: View {
    let editionLine: String
    let copyLine: String
    let records: [BridgeReleaseRecord]
    /// Whether any catalog describes the release, so the line offers the
    /// popover of them.
    let hasRecords: Bool
    /// Draw the hover plate as if the pointer were on the button, which a
    /// shot cannot do.
    var showsHover = false

    @State
    private var isHovering = false
    @State
    private var isShowingPopover = false

    var body: some View {
        if !hasRecords {
            factsText
        }
        else {
            Button {
                isShowingPopover = true
            } label: {
                HStack(
                    alignment: .firstTextBaseline,
                    spacing: ThemeSpace.inline
                ) {
                    factsText
                    Image(systemName: "arrow.up.right")
                        .themeText(.detail)
                        .foregroundStyle(Theme.textSecondary)
                        .allowsHitTesting(false)
                        .accessibilityLabel(
                            localizedCoreString("core.identity.identified")
                        )
                }
                .hoverPlate(
                    isHovering || showsHover,
                    cornerRadius: ThemeRadius.chip
                )
            }
            .buttonStyle(.accentPlain)
            .onHover { isHovering = $0 }
            .accessibilityIdentifier("release-facts")
            .macPopover(isPresented: $isShowingPopover, arrowEdge: .bottom) {
                ReleaseRecordsCard(records: records)
                    .accessibilityIdentifier("release-facts-card")
            }
        }
    }

    /// The lines the release states, each on one line of its own.
    var shownLines: [String] {
        [editionLine, copyLine].filter { !$0.isEmpty }
    }

    private var factsText: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.line) {
            ForEach(Array(shownLines.enumerated()), id: \.offset) { _, line in
                // Each line is a detail line of its own, as the board's are.
                Text(line)
                    .themeText(.detail)
                    .cutLine(.prose)
            }
        }
        .foregroundStyle(Theme.textTertiary)
    }
}
