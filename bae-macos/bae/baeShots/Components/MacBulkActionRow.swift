import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    static let macBulkActionRowScenes: [ShotScene] = [
        bulkActionRowScene(
            "mac.component.bulk-action-row.applies",
            action: .identify,
            count: 5
        ),
        bulkActionRowScene(
            "mac.component.bulk-action-row.strong",
            action: .import,
            count: 3
        ),
        bulkActionRowScene(
            "mac.component.bulk-action-row.applies-to-none",
            action: .cancelIdentification,
            count: 0
        ),
        bulkActionRowScene(
            "mac.component.bulk-action-row.with-destination",
            action: .import,
            count: 3,
            destination: true
        ),
    ]

    private static func bulkActionRowScene(
        _ id: String,
        action: BridgeCandidateAction,
        count: UInt64,
        destination: Bool = false
    ) -> ShotScene {
        component(
            id,
            width: 440,
            height: 60
        ) {
            importStores(
                MacBulkActionRow(
                    offer: ImportCandidateActionOffer(
                        BridgeSelectionOffer(
                            action: action,
                            count: count,
                            enabled: count > 0
                        ),
                        rowKey: nil
                    ),
                    importDestination: destination
                        ? .constant(.thisDevice) : nil,
                    action: {}
                )
                .disabled(count == 0)
                .frame(width: 440)
            )
        }
    }
}
