import BaeKit
import SwiftUI

/// Found's filter: one entry chosen at a time, each with how many of Found's
/// rows it holds. The entries, their order, their counts and which can be
/// chosen are core's.
///
/// `entries` is asked for in this view's body, which a menu evaluates when it
/// opens rather than when the list around it is drawn, so the counts are the
/// ones standing as the person looks.
struct PendingFilterSection: View {
    /// Every entry, in core's order.
    let entries: () -> [BridgePendingFilterEntry]
    let selected: BridgePendingFilter
    let onSelect: (BridgePendingFilter) -> Void

    /// One checkmark item per entry rather than an inline `Picker`: a
    /// picker's options stay enabled in the menu whatever `.disabled` says,
    /// and an entry holding no row cannot be chosen. Choosing the chosen
    /// entry again changes nothing.
    var body: some View {
        Section("Filter") {
            ForEach(entries(), id: \.filter) { entry in
                Toggle(
                    entry.filter.label(count: entry.count),
                    isOn: Binding(
                        get: { entry.filter == selected },
                        set: { chosen in
                            if chosen { onSelect(entry.filter) }
                        }
                    )
                )
                .disabled(!entry.selectable)
            }
        }
    }
}

extension BridgePendingFilter {
    var label: String {
        switch self {
        case .all: String(localized: "All")
        case .needsYou: String(localized: "Needs You")
        case .inProgress: String(localized: "In Progress")
        case .identified: String(localized: "Identified")
        case .unmatched: String(localized: "Unmatched")
        case .notLookedUp: String(localized: "Not Looked Up")
        }
    }

    /// The name with how many rows the entry holds, for the menu item, where
    /// the number has no place of its own.
    func label(count: UInt32) -> String {
        String(localized: "\(label) (\(Int(count)))")
    }
}
