import BaeKit
import SwiftUI

/// The states Found's rows can be narrowed to, each checked or not, under
/// All, which is checked while none narrows them. Every row is in exactly one
/// state. The states, their order and what checking one does to the rest are
/// core's.
struct PendingFilterSection: View {
    let filters: [BridgePendingState]
    let onSetFilter: (_ filter: BridgePendingState, _ checked: Bool) -> Void
    let onShowAll: () -> Void

    /// Core's groups, each set apart from the one before it.
    static let groups = bridgePendingStateGroups()

    var body: some View {
        Section("Filter") {
            Toggle(
                "All",
                isOn: Binding(
                    get: { filters.isEmpty },
                    set: { checked in
                        if checked { onShowAll() }
                    }
                )
            )
            ForEach(Self.groups, id: \.self) { group in
                Divider()
                ForEach(group, id: \.self) { filter in
                    Toggle(
                        filter.label,
                        isOn: Binding(
                            get: { filters.contains(filter) },
                            set: { onSetFilter(filter, $0) }
                        )
                    )
                }
            }
        }
    }
}

extension BridgePendingState {
    var label: String {
        switch self {
        case .notLookedUp: String(localized: "Not Looked Up")
        case .identifying: String(localized: "Identifying")
        case .needsYou: String(localized: "Needs You")
        case .identified: String(localized: "Identified")
        case .unmatched: String(localized: "Unmatched")
        case .lookupError: String(localized: "Lookup Error")
        case .error: String(localized: "Error")
        case .importing: String(localized: "Importing")
        case .importError: String(localized: "Import Error")
        }
    }
}
