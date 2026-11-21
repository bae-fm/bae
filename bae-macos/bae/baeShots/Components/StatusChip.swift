import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `StatusChip` in each tone.
    static let statusChipScenes: [ShotScene] = [
        statusChip("mac.component.status-chip.neutral", .neutral),
        statusChip("mac.component.status-chip.accent", .accent),
        statusChip("mac.component.status-chip.info", .info),
        statusChip("mac.component.status-chip.success", .success),
        statusChip("mac.component.status-chip.warning", .warning),
        statusChip("mac.component.status-chip.danger", .danger),
        statusChip("mac.component.status-chip.activity", .activity),
    ]

    private static func statusChip(_ id: String, _ tone: StatusTone)
        -> ShotScene
    {
        component(id, height: 40) {
            StatusChip(verbatim: "Synced", tone: tone)
        }
    }
}
