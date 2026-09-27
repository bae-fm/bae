import BaeKit
import SwiftUI

/// Which of Pending's rows the candidate list shows: all of them, or those
/// one of core's filters keeps. The filters and their order are core's.
struct PendingFilterPicker: View {
    let selection: BridgePendingFilter?
    let onSelect: (BridgePendingFilter?) -> Void

    private static let filters = bridgePendingFilters()

    var body: some View {
        Picker(
            "Filter",
            selection: Binding(get: { selection }, set: onSelect)
        ) {
            Text("All").tag(BridgePendingFilter?.none)
            ForEach(Self.filters, id: \.self) { filter in
                Text(filter.label).tag(Optional(filter))
            }
        }
        .pickerStyle(.inline)
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
