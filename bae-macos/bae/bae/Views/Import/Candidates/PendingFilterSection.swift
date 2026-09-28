import BaeKit
import SwiftUI

/// The states Pending's rows can be narrowed to, each checked or not, under
/// All, which is checked while none narrows them. The states, their order and
/// what checking one does to the rest are core's.
struct PendingFilterSection: View {
    let filters: [BridgePendingFilter]
    let onSetFilter: (_ filter: BridgePendingFilter, _ checked: Bool) -> Void
    let onShowAll: () -> Void

    /// Core's groups, each set apart from the one before it.
    static let groups = bridgePendingFilterGroups()

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

extension BridgePendingFilter {
    var label: String {
        switch self {
        case .identified: String(localized: "Identified")
        case .needsYou: String(localized: "Needs You")
        case .identifying: String(localized: "Identifying")
        case .importing: String(localized: "Importing")
        case .lookupError: String(localized: "Lookup Error")
        case .importError: String(localized: "Import Error")
        }
    }
}
