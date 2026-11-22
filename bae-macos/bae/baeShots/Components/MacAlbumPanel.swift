import AppKit
import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacAlbumPanel` for "Low Tide Hours", the first track playing: at
    /// rest, with a CD and an LP, rated by the person and by Ana, on another
    /// Mac's disk, and with no cover.
    static let macAlbumPanelScenes: [ShotScene] = [
        albumPanel("mac.component.album-panel.resting", detail: panelDetail()),
        albumPanel(
            "mac.component.album-panel.two-releases",
            detail: panelDetail(releases: ["CD", "LP"])
        ),
        albumPanel(
            "mac.component.album-panel.rated",
            detail: panelDetail(rating: ratedByYouAndAna)
        ),
        albumPanel(
            "mac.component.album-panel.unplayable",
            detail: panelDetail(playability: onAnasMac)
        ),
        albumPanel(
            "mac.component.album-panel.no-cover",
            detail: panelDetail(covered: false)
        ),
    ]

    /// The album the panel and the track list draw: "Low Tide Hours" on a
    /// release named `name`, side A with two tracks.
    private static func panelRelease(
        id: String,
        name: String,
        medium: BridgeMedium,
        covered: Bool,
        playability: BridgeReleasePlayability
    ) -> BridgeRelease {
        var release = PreviewData.boardAlbumDetail().releases[0]
        let facts = BridgePressingFacts(
            area: .country(code: "US"),
            media: PreviewData.media(medium),
            status: nil,
            packaging: nil,
            discogsDetails: []
        )
        release.id = id
        release.name = .named(name: name)
        release.facts = facts
        release.pressingSummary = bridgePressingSummary(facts: facts)
        release.mediaTerms = bridgeMediaTerms(media: facts.media)
        release.pressingDetails = bridgePressingDetails(facts: facts)
        release.labels = [
            BridgeLabelLine(
                names: ["Harbor Records"],
                catalogNumbers: []
            )
        ]
        release.cover = covered ? cover("Low Tide Hours") : nil
        release.playability = playability
        let tracks = [
            browseTrack(
                "a1",
                "Harbor Lights",
                position: "A1",
                minutes: 4,
                seconds: 32
            ),
            browseTrack(
                "a2",
                "The Long Way Home",
                position: "A2",
                minutes: 5,
                seconds: 12
            ),
        ]
        release.tracks = tracks
        release.trackGroups = [
            BridgeTrackGroup(
                side: .sided(sideLetter: "A"),
                tracks: tracks,
                totalDuration: nil,
                showsTime: false,
                secondColumnStart: nil
            )
        ]
        return release
    }

    /// "Low Tide Hours" with one release per name in `releases`, the one at
    /// `shown` showing; the first is a CD, the others LPs.
    static func panelDetail(
        releases names: [String] = ["CD"],
        shown: Int = 0,
        rating: BridgeAlbumRating = BridgeAlbumRating(mine: nil, raters: []),
        playability: BridgeReleasePlayability = .playable,
        year: Int32? = 1992,
        covered: Bool = true,
        reissueYear: Int32? = nil
    ) -> AlbumDetail {
        var detail = PreviewData.boardAlbumDetail()
        var releases = names.enumerated()
            .map { index, name in
                panelRelease(
                    id: "rel-\(index)",
                    name: name,
                    medium: index == 0 ? .cd : .vinyl,
                    covered: covered,
                    playability: playability
                )
            }
        releases[0].distinctYear = reissueYear
        detail.album.releaseIds = releases.map(\.id)
        detail.album.year = year
        detail.album.cover = covered ? cover("Low Tide Hours") : nil
        detail.releases = releases
        detail.rating = rating
        detail.showsReleaseChoices = releases.count > 1
        return AlbumDetail(
            bridge: detail.showing(chosenReleaseId: "rel-\(shown)")
        )
    }

    private static func albumPanel(
        _ id: String,
        detail: AlbumDetail
    ) -> ShotScene {
        component(
            id,
            width: 804,
            height: 480,
            images: browsePanelImages
        ) {
            MacAlbumPanel(
                detail: detail,
                onBrowseImages: {},
                onSelectRelease: { _ in },
                currentTrackId: "a1",
                currentReleaseId: nil,
                isPlaying: true,
                onClose: {},
                onPlay: {},
                onShuffle: {},
                onPlayFromTrack: { _ in },
                onTogglePlayPause: {},
                onAddNext: { _ in },
                onAddToQueue: { _ in },
                onAddNextAlbum: {},
                onAddAlbumToQueue: {},
                onEdit: {},
                onOpenStorage: {},
                onExportRelease: {},
                onSaveReleaseAs: {},
                onSetPrimaryRelease: {},
                onDeleteRelease: {},
                onDeleteAlbum: {},
                onExportTrack: { _ in },
                onOpenWork: { _ in },
                onRateAlbum: { _ in },
                onRateTrack: { _, _ in }
            )
            .albumDetailPreviewEnvironment()
            .environment(browsePanelImages)
        }
    }

    /// Four stars from the person, and Ana's five.
    static let ratedByYouAndAna = BridgeAlbumRating(
        mine: .four,
        raters: [.you, .person(name: "Ana", stars: .five)]
    )

    /// A release whose audio is on Ana's Mac.
    static let onAnasMac = BridgeReleasePlayability.onOtherDevice(
        device: .active(name: "Ana’s Mac")
    )

    private static let browsePanelImages = coverStore()
}
