import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    static let macPressingRowScenes: [ShotScene] = [
        pressingRowScene("mac.component.pressing-row.plain", cdPressing()),
        pressingRowScene(
            "mac.component.pressing-row.badges",
            cdPressing(),
            agreements: agreements()
        ),
        pressingRowScene(
            "mac.component.pressing-row.selected",
            cdPressing(chosen: true),
            agreements: agreements()
        ),
        pressingRowScene(
            "mac.component.pressing-row.in-library",
            cdPressing(inLibrary: true)
        ),
        pressingRowScene(
            "mac.component.pressing-row.records-differ",
            differingPressing(),
            agreements: agreements(
                catalog: true,
                notes: [
                    BridgeTextStatement(
                        text: "first pressing",
                        catalogs: [.musicBrainz]
                    ),
                    BridgeTextStatement(
                        text: "club edition",
                        catalogs: [.discogs]
                    ),
                ]
            ),
            height: 100
        ),
        pressingRowScene(
            "mac.component.pressing-row.notes",
            cdPressing(),
            agreements: agreements(
                notes: [
                    BridgeTextStatement(text: "first pressing", catalogs: [])
                ]
            ),
            height: 90
        ),
        pressingRowScene(
            "mac.component.pressing-row.year-unknown",
            pressing(
                releaseId: "hrlp-1042",
                year: nil,
                catalog: nil,
                facts: PreviewData.pressingFacts(
                    media: PreviewData.media(.cd)
                ),
                sources: [.discogs]
            ),
            height: 70
        ),
    ]

    /// The same CD where MusicBrainz and Discogs state its year and notes
    /// differently, and both state the one country.
    private static func differingPressing() -> BridgePressing {
        let cd = PreviewData.pressingFacts(
            country: "US",
            media: PreviewData.media(.cd)
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
            sources: [.musicBrainz, .discogs],
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
            barcodes: [
                BridgeTextStatement(text: "0 41234 56789 2", catalogs: [])
            ],
            documentFailure: nil,
            pick: pick("hr-1042", .musicBrainz),
            areas: [
                BridgeTermsStatement(
                    terms: [.country(code: "US")],
                    catalogs: []
                )
            ],
            media: [
                BridgeTermsStatement(
                    terms: bridgeMediaTerms(media: cd.media),
                    catalogs: []
                )
            ],
            details: [],
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
            MacPressingRow(
                pressing: pressing,
                isImporting: false,
                agreements: agreements,
                isSelected: bridge.chosen,
                onSelect: { _ in }
            )
        }
    }

    private static func pressingRowScene(
        _ id: String,
        _ bridge: BridgePressing,
        agreements: BridgeAgreements? = nil,
        height: CGFloat = 80
    ) -> ShotScene {
        component(
            id,
            width: 560,
            height: height
        ) {
            pressingRow(bridge, agreements: agreements)
                .frame(width: 560, alignment: .leading)
        }
    }
}
