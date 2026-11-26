import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacPressingTable`: two albums' pressings with a column for the Disc ID
    /// and the barcode, and without identifier columns.
    static let macPressingTableScenes: [ShotScene] = [
        pressingTableScene(
            "mac.component.pressing-table.identifier-columns",
            kinds: [.discId, .barcode]
        ),
        pressingTableScene(
            "mac.component.pressing-table.no-identifier-columns",
            kinds: []
        ),
    ]

    /// The 1993 Japanese CD, on a label with a long name.
    private static func japaneseCdPressing() -> BridgePressing {
        pressing(
            releaseId: "hrcd-1042",
            year: 1993,
            label: "Harbor Records under license to Tidewater Music",
            catalog: "HRCD-1042",
            facts: PreviewData.pressingFacts(
                country: "Japan",
                media: PreviewData.media(.cd),
                discogsDetails: [.limitedEdition, .numbered]
            ),
            sources: [.musicBrainz, .discogs]
        )
    }

    /// The 2014 remastered LP reissue.
    private static func laterLpPressing() -> BridgePressing {
        pressing(
            releaseId: "twlp-0412",
            year: 2014,
            label: "Tidewater Music",
            catalog: "TWLP-0412",
            facts: PreviewData.pressingFacts(
                region: .europe,
                media: PreviewData.media(.vinyl),
                discogsDetails: [.lp, .reissue, .remastered]
            ),
            sources: [.musicBrainz, .discogs]
        )
    }

    private static func pressingTableScene(
        _ id: String,
        kinds: [PressingKind]
    ) -> ShotScene {
        let later = lowTideHoursGroup(
            titles: ["Low Tide Hours (Remastered)"],
            pressings: [laterLpPressing()]
        )
        let first = lowTideHoursGroup(pressings: [
            cdPressing(chosen: true, barcode: nil),
            japaneseCdPressing(),
        ])
        return component(
            id,
            width: 842,
            height: 280,
            images: coverGalleryImages()
        ) {
            MacPressingTable(kinds: kinds) {
                VStack(alignment: .leading, spacing: ThemeSpace.section) {
                    MacReleaseGroup(
                        group: first,
                        isImporting: false,
                        agreements: [
                            recordId("hr-1042", .musicBrainz): agreements(),
                            recordId("hrcd-1042", .musicBrainz):
                                BridgeAgreements(
                                    discId: false,
                                    barcode: true,
                                    catalog: false,
                                    label: false,
                                    year: false,
                                    country: false,
                                    notes: []
                                ),
                        ],
                        onSelect: { _ in }
                    )
                    MacReleaseGroup(
                        group: later,
                        isImporting: false,
                        onSelect: { _ in }
                    )
                }
            }
            .frame(width: 900, alignment: .leading)
            // The preview's table is 900 points wide in a 842 point cell, which
            // cuts it at the cell's edge.
            .frame(width: 842, alignment: .leading)
            .clipped()
        }
    }
}
