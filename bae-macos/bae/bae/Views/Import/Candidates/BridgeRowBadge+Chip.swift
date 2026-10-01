import BaeKit
import SwiftUI

extension BridgePendingBadge {
    /// The few words the badge says about why the row waits on the person.
    var label: String {
        switch self {
        case .needsYou(let reason):
            reason.badgeLabel
        case .lookupError:
            String(localized: "Lookup error")
        case .error:
            String(localized: "Error")
        case .importError:
            String(localized: "Import error")
        }
    }
}

extension BridgeBadgeTone {
    var statusTone: StatusTone {
        switch self {
        case .attention: .warning
        case .failure: .danger
        }
    }
}

/// The badge core gave a Found row waiting on the person.
struct RowBadgeChip: View {
    let badge: BridgeRowBadge

    var body: some View {
        StatusChip(verbatim: badge.says.label, tone: badge.tone.statusTone)
    }
}
