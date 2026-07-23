import BaeKit
import SwiftUI

/// The word for a sync-indicator state. Core decides which state wins
/// (`BridgeSyncIndicator`); this only names it — the rendering that stays in the
/// UI. `Synced` reads its time elsewhere; the toolbar renders a spinner for
/// `Syncing` rather than this word.
enum SyncIndicatorLabel {
    static func text(_ indicator: BridgeSyncIndicator) -> String {
        switch indicator {
        case .synced:
            String(localized: "Synced")
        case .syncing, .idle:
            String(localized: "Syncing\u{2026}")
        case .error:
            String(localized: "Sync error")
        }
    }
}
