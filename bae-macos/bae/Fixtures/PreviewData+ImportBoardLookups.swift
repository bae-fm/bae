import BaeKit
import Foundation

@testable import bae

/// The import boards of Low Tide Hours while Find online's run waits on a
/// lookup, and after one failed.
extension PreviewData {
    /// How the run of Low Tide Hours stands.
    enum LowTideHoursLookup {
        case searching
        case failed
    }

    /// The row's live state in the Found list: identifying while the run is
    /// out, a lookup error once one failed; none for another pane.
    static func lowTideHoursLookupLive(
        _ pane: FoundPane
    ) -> BridgeTriageListLiveState? {
        switch pane {
        case .lowTideHoursSearching:
            triageLive(boardDraftActions, identifying: .running)
        case .lowTideHoursLookupFailed:
            triageLive(
                boardDraftActions,
                badge: BridgeRowBadge(says: .lookupError, tone: .failure)
            )
        default:
            nil
        }
    }

    /// What the run has: the Disc ID and the barcode found releases, and the
    /// catalog number either still asks or found one too.
    private static func lowTideHoursLookupRun(
        _ lookup: LowTideHoursLookup
    ) -> BridgeIdentifyRun {
        let searching = lookup == .searching
        return BridgeIdentifyRun(
            providers: [.musicBrainz, .discogs],
            discId: .read(discs: [
                BridgeDiscIdRow(
                    discId: "v8n2Kq1Z…",
                    lookup: .noMatch,
                    chip: .init(look: .found, trailing: .nothing)
                )
            ]),
            barcode: .rows(rows: [
                BridgeSignalValueRow(
                    value: "0 41234 56789 2",
                    excluded: false,
                    chip: .init(
                        look: .found,
                        trailing: searching
                            ? .searching : .failed(failure: .timeout)
                    )
                )
            ]),
            catalog: .numbers(
                rows: [
                    BridgeSignalValueRow(
                        value: "HR-1042",
                        excluded: false,
                        chip: .init(
                            look: searching ? .on : .found,
                            trailing: searching ? .searching : .nothing
                        )
                    )
                ],
                candidates: []
            ),
            isrc: .absent,
            search: .notNeeded,
            foundKinds: searching
                ? [.discId, .barcode] : [.discId, .barcode, .catalog],
            readingArtwork: false
        )
    }

    private static let lowTideHoursRunGroupId = "board-low-tide-hours-run"

    /// One of the run's pressings, by Harbor Records.
    private static func lowTideHoursRunPressing(
        id: String,
        year: Int32,
        catalog: String,
        facts: BridgePressingFacts,
        inLibrary: Bool = false
    ) -> BridgePressing {
        boardMusicBrainzPressing(
            boardMusicBrainzRecord(
                id: id,
                year: year,
                facts: facts,
                groupId: lowTideHoursRunGroupId
            ),
            label: "Harbor Records",
            catalog: catalog,
            inLibrary: inLibrary
        )
    }

    /// What the run found: the album in two pressings, and a third once a
    /// lookup failed.
    private static func lowTideHoursLookupGroup(
        _ lookup: LowTideHoursLookup
    ) throws -> BridgeReleaseGroup {
        let cd = pressingFacts(country: "US", media: media(.cd))
        var pressings = [
            lowTideHoursRunPressing(
                id: "run-cd-us",
                year: 1992,
                catalog: "HR-1042",
                facts: cd
            )
        ]
        if lookup == .failed {
            pressings.append(
                lowTideHoursRunPressing(
                    id: "run-cd-eu",
                    year: 1992,
                    catalog: "HR-1042",
                    facts: pressingFacts(region: .europe, media: media(.cd))
                )
            )
        }
        pressings.append(
            lowTideHoursRunPressing(
                id: "run-vinyl",
                year: 2014,
                catalog: "HRLP-1042",
                facts: pressingFacts(
                    media: media(.vinyl),
                    discogsDetails: [.reissue]
                ),
                inLibrary: true
            )
        )
        return BridgeReleaseGroup(
            id: lowTideHoursRunGroupId,
            titles: ["Low Tide Hours"],
            artists: [.named("Mara Quill")],
            // The catalog serves the cover as a file the scene's image store
            // reads, so the card draws it as the board does.
            coverArt: BridgeRemoteCover(
                coverChoice: try catalogCover(named: "Low Tide Hours"),
                sourceName: "Cover Art Archive",
                label: .lead(releaseGroup: true)
            ),
            sources: [
                BridgeReleaseGroupSource(
                    source: .musicBrainz,
                    groupUrl:
                        "https://musicbrainz.org/release-group/\(lowTideHoursRunGroupId)",
                    albumLinksUnread: false
                )
            ],
            sections: [
                BridgePressingSection(
                    album: nil,
                    pressings: pressings,
                    narrowedOut: []
                )
            ]
        )
    }

    /// What identification has for Low Tide Hours in flight.
    static func lowTideHoursLookupRuntime(
        _ lookup: LowTideHoursLookup
    ) throws -> BridgeCandidateRuntimeSnapshot {
        let run = lowTideHoursLookupRun(lookup)
        let group = try lowTideHoursLookupGroup(lookup)
        let state: BridgeIdentifyState =
            switch lookup {
            case .searching:
                .triangulating(
                    run: run,
                    groups: [group],
                    agreements: lowTideHoursAgreements(lookup),
                    narrowedOutCount: 0
                )
            case .failed:
                .failed(
                    run: run,
                    failureLines: nil,
                    groups: [group],
                    agreements: lowTideHoursAgreements(lookup),
                    narrowedOutCount: 0,
                    offersSharedAlbum: false
                )
            }
        return BridgeCandidateRuntimeSnapshot(
            identifyState: .state(state: state),
            inFlight: nil
        )
    }

    /// Which identifiers name each pressing: the dots of the table.
    private static func lowTideHoursAgreements(
        _ lookup: LowTideHoursLookup
    ) -> [String: BridgeAgreements] {
        func named(
            discId: Bool = false,
            barcode: Bool = false,
            catalog: Bool = false
        ) -> BridgeAgreements {
            BridgeAgreements(
                discId: discId,
                barcode: barcode,
                catalog: catalog,
                label: false,
                year: false,
                country: false,
                notes: []
            )
        }
        switch lookup {
        case .searching:
            return ["run-cd-us": named(discId: true, barcode: true)]
        case .failed:
            return [
                "run-cd-us": named(discId: true, barcode: true, catalog: true),
                "run-cd-eu": named(catalog: true),
            ]
        }
    }
}
