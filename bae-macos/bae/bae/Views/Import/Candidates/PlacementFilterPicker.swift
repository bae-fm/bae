import BaeKit
import SwiftUI

/// Which of Pending's rows the candidate list shows, by where core places
/// each one: the placements rows are sectioned by, and within Needs you each
/// question a row can ask. Every choice is core's own; this names them.
struct PlacementFilterPicker: View {
    let selection: BridgePlacementFilter
    let onSelect: (BridgePlacementFilter) -> Void

    /// The questions a Needs-you row can ask, in the order a list reads them.
    private static let questions: [BridgeNeedsYouKind] = [
        .severalMatches, .noMatch, .nothingToLookUp, .lookupFailed,
        .trackCountDisagrees, .sourceTracksUnknown,
    ]

    var body: some View {
        Picker(
            "Show",
            selection: Binding(get: { selection }, set: onSelect)
        ) {
            Text("All Candidates").tag(BridgePlacementFilter.any)
            Text("Ready to Import").tag(BridgePlacementFilter.ready)
            Text("Needs You").tag(BridgePlacementFilter.needsYou(kind: nil))
            ForEach(Self.questions, id: \.self) { kind in
                Text(kind.filterLabel)
                    .tag(BridgePlacementFilter.needsYou(kind: kind))
            }
            Text("Import Failed").tag(BridgePlacementFilter.failed)
            Text("Not Answered Yet").tag(BridgePlacementFilter.unanswered)
        }
        .pickerStyle(.inline)
    }
}

extension BridgeNeedsYouKind {
    /// The question as a filter names it, indented under Needs You.
    var filterLabel: String {
        switch self {
        case .severalMatches: String(localized: "Several Matches")
        case .noMatch: String(localized: "Nothing Matched")
        case .nothingToLookUp: String(localized: "Nothing to Look Up")
        case .lookupFailed: String(localized: "Lookup Failed")
        case .trackCountDisagrees: String(localized: "Track Count Differs")
        case .sourceTracksUnknown: String(localized: "No Tracklist")
        }
    }
}
