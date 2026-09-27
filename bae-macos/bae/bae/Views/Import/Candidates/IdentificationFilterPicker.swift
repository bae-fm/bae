import BaeKit
import SwiftUI

/// Which of Pending's rows the candidate list shows, by what each one's
/// stored lookup result says: all of them, or those with one outcome. The
/// outcomes and their order are core's; this names them.
struct IdentificationFilterPicker: View {
    let selection: BridgeIdentificationOutcome?
    let onSelect: (BridgeIdentificationOutcome?) -> Void

    private static let outcomes = bridgeIdentificationOutcomes()

    var body: some View {
        Picker(
            "Identification",
            selection: Binding(get: { selection }, set: onSelect)
        ) {
            Text("All").tag(BridgeIdentificationOutcome?.none)
            ForEach(Self.outcomes, id: \.self) { outcome in
                Text(outcome.filterLabel).tag(Optional(outcome))
            }
        }
        .pickerStyle(.inline)
    }
}

extension BridgeIdentificationOutcome {
    /// The outcome as the Identification filter names it.
    var filterLabel: String {
        switch self {
        case .notIdentified: String(localized: "Not Identified")
        case .oneRelease: String(localized: "One Release")
        case .severalReleases: String(localized: "Several Releases")
        case .noMatch: String(localized: "No Match")
        case .lookupFailed: String(localized: "Lookup Failed")
        }
    }
}
