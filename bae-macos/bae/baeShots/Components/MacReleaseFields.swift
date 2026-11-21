import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    static let macReleaseFieldsScenes: [ShotScene] = [
        releaseFieldsScene(
            "mac.component.release-fields.filled",
            release(
                pressing: pressing(
                    barcode: "0 41234 56789 2",
                    packaging: .jewelCase,
                    details: [.remastered]
                )
            )
        ),
        releaseFieldsScene(
            "mac.component.release-fields.several-details",
            release(
                pressing: pressing(
                    barcode: "0 41234 56789 2",
                    packaging: .jewelCase,
                    details: [.remastered, .limitedEdition]
                )
            )
        ),
        releaseFieldsScene(
            "mac.component.release-fields.partly-filled",
            release(pressing: pressing())
        ),
        releaseFieldsScene(
            "mac.component.release-fields.long-values",
            release(
                pressing: pressing(
                    label: "Harbor Records and the Northern Sound Company",
                    catalog: "HR-1042-2-REISSUE-DELUXE",
                    barcode: "0 41234 56789 2",
                    packaging: .jewelCase,
                    details: [.limitedEdition, .reissue, .remastered]
                )
            )
        ),
        releaseFieldsScene("mac.component.release-fields.empty", blankEdit()),
        releaseFieldsScene(
            "mac.component.release-fields.country-menu-open",
            release(
                pressing: pressing(
                    barcode: "0 41234 56789 2",
                    packaging: .jewelCase,
                    details: [.remastered]
                )
            )
        ),
    ]

    /// The release Low Tide Hours by Mara Quill with the pressing `pressing`.
    private static func release(
        pressing: BridgeRawPressingEdit
    ) -> BridgeRawReleaseEdit {
        PreviewData.boardEdit(
            title: "Low Tide Hours",
            artist: "Mara Quill",
            year: "1992",
            pressing: pressing,
            tracks: []
        )
    }

    private static func releaseFieldsScene(
        _ id: String,
        _ values: BridgeRawReleaseEdit
    ) -> ShotScene {
        component(
            id,
            width: 560,
            height: 300
        ) {
            importStores(
                MacReleaseFields(
                    values: values,
                    pressingOffers: PreviewData.pressingOffers,
                    writer: ReleaseFieldWriter { _, _ in },
                    editingCommands: EditingCommitCommands()
                )
                .frame(width: 560, alignment: .leading)
            )
        }
    }
}
