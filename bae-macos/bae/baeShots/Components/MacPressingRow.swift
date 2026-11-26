import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    static let macPressingRowScenes: [ShotScene] = [
        pressingRowScene("mac.component.pressing-row.plain", clubCdPressing()),
        pressingRowScene(
            "mac.component.pressing-row.matches",
            clubCdPressing(),
            agreements: agreements()
        ),
        pressingRowScene(
            "mac.component.pressing-row.selected",
            clubCdPressing(chosen: true),
            agreements: agreements()
        ),
        pressingRowScene(
            "mac.component.pressing-row.in-library",
            clubCdPressing(inLibrary: true)
        ),
        pressingRowScene(
            "mac.component.pressing-row.records-differ",
            differingPressing()
        ),
        pressingRowScene(
            "mac.component.pressing-row.notes",
            clubCdPressing(),
            agreements: agreesInNothing(
                notes: [
                    BridgeTextStatement(text: "first pressing", catalogs: [])
                ]
            )
        ),
        pressingRowScene(
            "mac.component.pressing-row.year-unknown",
            pressing(
                releaseId: "hrlp-1042",
                year: nil,
                catalog: "HR-1042",
                facts: PreviewData.pressingFacts(
                    country: "US",
                    media: PreviewData.media(.cd),
                    discogsDetails: [.clubEdition]
                ),
                sources: [.musicBrainz, .discogs],
                barcode: sampleBarcode
            )
        ),
    ]

    /// The 1992 CD as a club edition.
    private static func clubCdPressing(
        chosen: Bool = false,
        inLibrary: Bool = false
    ) -> BridgePressing {
        cdPressing(
            chosen: chosen,
            inLibrary: inLibrary,
            details: [.clubEdition]
        )
    }

    /// The same CD where MusicBrainz and Discogs state its year and notes
    /// differently, and both state the one country.
    private static func differingPressing() -> BridgePressing {
        let cd = PreviewData.pressingFacts(
            country: "US",
            media: PreviewData.media(.cd),
            discogsDetails: [.clubEdition]
        )
        return BridgePressing(
            releases: [
                record(
                    .musicBrainz,
                    releaseId: "hr-1042",
                    year: 1992,
                    facts: cd,
                    barcode: "0 41234 56789 2"
                ),
                record(
                    .discogs,
                    releaseId: "hr-1042",
                    year: 1993,
                    facts: cd,
                    barcode: "0 41234 56789 2"
                ),
            ],
            labels: [
                BridgeStatedLabelLine(
                    names: ["Harbor Records"],
                    catalogNumbers: [
                        BridgeTextStatement(text: "HR-1042", catalogs: [])
                    ]
                )
            ],
            years: [
                BridgeYearStatement(year: 1992, catalogs: [.musicBrainz]),
                BridgeYearStatement(year: 1993, catalogs: [.discogs]),
            ],
            documentFailure: nil,
            pick: pick("hr-1042", .musicBrainz),
            areas: [
                BridgeTermsStatement(
                    terms: bridgeAreaTerms(facts: cd),
                    catalogs: []
                )
            ],
            media: [
                BridgeTermsStatement(
                    terms: bridgeMediaTerms(media: cd.media),
                    catalogs: []
                )
            ],
            details: bridgePressingDetails(facts: cd),
            chosen: false,
            pickBeingRead: false,
            inLibrary: false
        )
    }

    @ViewBuilder
    private static func pressingRow(
        _ bridge: BridgePressing,
        agreements: BridgeAgreements? = nil
    ) -> some View {
        if let pressing = Pressing(bridge: bridge) {
            MacPressingTable(kinds: PressingKind.allCases) {
                MacPressingRow(
                    pressing: pressing,
                    isImporting: false,
                    agreements: agreements,
                    isSelected: bridge.chosen,
                    onSelect: { _ in }
                )
            }
        }
    }

    private static func pressingRowScene(
        _ id: String,
        _ bridge: BridgePressing,
        agreements: BridgeAgreements? = nil,
        height: CGFloat = 52
    ) -> ShotScene {
        component(
            id,
            width: 842,
            height: height
        ) {
            pressingRow(bridge, agreements: agreements)
                .frame(width: 900, alignment: .leading)
                // The preview's table is 900 points wide in a 842 point cell, which
                // cuts it at the cell's edge.
                .frame(width: 842, alignment: .leading)
                .clipped()
        }
    }
}
