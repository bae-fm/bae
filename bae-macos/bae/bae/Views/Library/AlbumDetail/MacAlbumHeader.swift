import BaeKit
import SwiftUI

/// The open album's head: title, artist and year, the release picker when the
/// album has several releases, the release's facts, why this Mac can't play
/// it, then Play and ⋯ over the album's rating. Under Heart a filled heart
/// follows the ⋯ when the person hearted the album; under None the album
/// shows no rating.
struct MacAlbumHeader: View {
    /// The album and its releases as its detail read returned them.
    let detail: AlbumDetail
    /// The release of the track playing now, which the release picker marks.
    let currentReleaseId: String?
    /// Show the release with this id, which the person picked in the release
    /// picker.
    let onSelectRelease: (String) -> Void
    let onPlay: () -> Void
    /// Rate the album with these stars, or clear its rating with none.
    let onRate: (BridgeStars?) -> Void
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
        onRate: @escaping (BridgeStars?) -> Void,
        rating: BridgeAlbumRating,
        @MacMenuBuilder menu: @escaping () -> [MacMenuEntry]
    ) {
        self.detail = detail
        self.currentReleaseId = currentReleaseId
        self.onSelectRelease = onSelectRelease
        self.onPlay = onPlay
        self.onRate = onRate
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
                    .foregroundStyle(Theme.macTextSecondary)
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
                pressingLine: selectedRelease.pressingLine,
                labelsLine: selectedRelease.labelsLine,
                records: selectedRelease.records,
                hasRecords: selectedRelease.hasRecords
            )
            // Why this Mac can't play the release: the device its audio is
            // on, or its originals missing here.
            if let unplayable = selectedRelease.unplayableLine {
                Text(unplayable)
                    .themeText(.detail)
                    .foregroundStyle(Theme.macTextSecondary)
            }
            HStack(spacing: ThemeSpace.related) {
                MacButton("Play", kind: .primary, action: onPlay)
                MacMoreMenu(
                    look: .boxed,
                    label: Text("Album options"),
                    items: menu
                )
                // Under Heart the person's heart follows the ⋯, read-only.
                if case .heart(hearted: true, hearters: _) = rating {
                    Heart(on: true, size: 13)
                }
            }
            .padding(.top, ThemeSpace.group)
            if case .stars(let mine, let raters) = rating {
                MacAlbumRating(mine: mine, raters: raters, onRate: onRate)
                    .padding(.top, ThemeSpace.related)
            }
        }
        // Over the rows below the head, which the facts' card lies across.
        .zIndex(1)
    }
}
