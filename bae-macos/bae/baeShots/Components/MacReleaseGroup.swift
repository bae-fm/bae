import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacReleaseGroup`: the album over its one pressing, picked, and over
    /// three, the last in the library.
    static let macReleaseGroupScenes: [ShotScene] = [
        releaseGroupScene(
            "mac.component.release-group.one-pressing",
            lowTideHoursGroup(pressings: [
                cdPressing(chosen: true, barcode: nil)
            ]),
            agreements: [recordId("hr-1042", .musicBrainz): agreements()],
            height: 150
        ),
        releaseGroupScene(
            "mac.component.release-group.several-pressings",
            lowTideHoursGroup(pressings: [
                cdPressing(barcode: nil), europeanCdPressing(),
                lpReissuePressing(),
            ]),
            agreements: [
                recordId("hr-1042", .musicBrainz): agreements(),
                recordId("hr-1042-eu", .musicBrainz): BridgeAgreements(
                    discId: false,
                    barcode: false,
                    catalog: true,
                    label: false,
                    year: false,
                    country: false,
                    notes: []
                ),
            ],
            height: 270
        ),
    ]

    /// The 1992 CD as Europe's pressing, which the folder's catalog number
    /// agrees with.
    private static func europeanCdPressing() -> BridgePressing {
        pressing(
            releaseId: "hr-1042-eu",
            year: 1992,
            catalog: "HR-1042",
            facts: PreviewData.pressingFacts(
                region: .europe,
                media: PreviewData.media(.cd)
            ),
            sources: [.musicBrainz, .discogs]
        )
    }

    /// The 2014 LP reissue, which the library holds.
    private static func lpReissuePressing() -> BridgePressing {
        pressing(
            releaseId: "hrlp-1042",
            year: 2014,
            catalog: "HRLP-1042",
            facts: PreviewData.pressingFacts(
                media: PreviewData.media(.vinyl),
                discogsDetails: [.reissue]
            ),
            sources: [.musicBrainz, .discogs],
            inLibrary: true
        )
    }

    private static func releaseGroupScene(
        _ id: String,
        _ group: ReleaseGroup,
        agreements: [String: BridgeAgreements],
        height: CGFloat
    ) -> ShotScene {
        component(
            id,
            width: wideColumn,
            height: height,
            images: coverGalleryImages()
        ) {
            MacReleaseGroup(
                group: group,
                isImporting: false,
                agreements: agreements,
                onSelect: { _ in }
            )
            .frame(width: wideColumn, alignment: .leading)
        }
    }
}
