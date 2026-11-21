import BaeKit
import SwiftUI

/// The open album's head: title, artist and year, the release picker when the
/// album has several releases, the release's facts, why this Mac can't play
/// it, then Play and ⋯ with the person's rating after it: under Stars the
/// number and one star, under Heart a filled heart when the person hearted
/// the album, and under None nothing.
struct MacAlbumHeader: View {
    /// The album and its releases as its detail read returned them.
    let detail: AlbumDetail
    /// The release of the track playing now, which the release picker marks.
    let currentReleaseId: String?
    /// Show the release with this id, which the person picked in the release
    /// picker.
    let onSelectRelease: (String) -> Void
    let onPlay: () -> Void
    /// The album's rating under the library's rating system, which the
    /// heart the person just chose can replace.
    let rating: BridgeAlbumRating
    /// The items of the ⋯ menu.
    let menu: () -> [MacMenuEntry]

    init(
        detail: AlbumDetail,
        currentReleaseId: String?,
        onSelectRelease: @escaping (String) -> Void,
        onPlay: @escaping () -> Void,
        rating: BridgeAlbumRating,
        @MacMenuBuilder menu: @escaping () -> [MacMenuEntry]
    ) {
        self.detail = detail
        self.currentReleaseId = currentReleaseId
        self.onSelectRelease = onSelectRelease
        self.onPlay = onPlay
        self.rating = rating
        self.menu = menu
    }

    private var album: BridgeAlbum { detail.album }

    /// The release the page shows, as its detail read chose it.
    private var selectedRelease: ReleaseDetail { detail.shownRelease }

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.inline) {
            Text(album.title)
                .themeText(.hero)
                .lineLimit(1)
            HStack(spacing: ThemeSpace.related) {
                Text(album.artistLine.text)
                    .foregroundStyle(Theme.textSecondary)
                if let year = album.year {
                    Text(verbatim: FactList.glyph)
                        .foregroundStyle(Theme.textTertiary)
                    Text(PressingText.year(year))
                        .foregroundStyle(Theme.textTertiary)
                }
            }
            .themeText(.heading)
            .lineLimit(1)
            if detail.showsReleaseChoices {
                MacReleasePicker(
                    detail: detail,
                    currentReleaseId: currentReleaseId,
                    onSelect: onSelectRelease
                )
            }
            ReleaseFactsLine(
                editionLine: selectedRelease.editionLine,
                copyLine: selectedRelease.copyLine,
                records: selectedRelease.records,
                hasRecords: selectedRelease.hasRecords
            )
            // Why this Mac can't play the release: the device its audio is
            // on, or its originals missing here.
            if let unplayable = selectedRelease.unplayableLine {
                Text(unplayable)
                    .themeText(.detail)
                    .foregroundStyle(Theme.textSecondary)
            }
            HStack(spacing: ThemeSpace.related) {
                MacButton("Play", kind: .primary, action: onPlay)
                MacMoreMenu(
                    look: .boxed,
                    label: Text("Album options"),
                    items: menu
                )
                ratingMark
            }
            .padding(.top, ThemeSpace.group)
        }
        // Over the rows below the head, which the facts' card lies across.
        .zIndex(1)
    }

    /// The person's own rating after the ⋯, read-only.
    @ViewBuilder
    private var ratingMark: some View {
        switch rating {
        case .stars(mine: let mine?, raters: _):
            Stars(rating: mine, size: 13)
                .padding(.leading, ThemeSpace.inline)
        case .heart(hearted: true, hearters: _):
            Heart(on: true, size: 13)
                .padding(.leading, ThemeSpace.inline)
        case .stars, .heart, .hidden:
            EmptyView()
        }
    }
}
