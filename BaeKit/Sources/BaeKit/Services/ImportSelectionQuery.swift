import Foundation

// The import list is a desktop surface: bae-bridge compiles the import list's
// types only for desktop targets, so the iOS bindings do not have them.
#if os(macOS)

    /// A live query reads what the rows the import list's screen keeps
    /// selected hold and offer, whatever the list shows. The caller changes
    /// the selected keys on the same query, and each value answers for the
    /// keys the caller set last.
    public struct ImportSelectionQuery: Sendable {
        public let setSelected: @Sendable ([Data]) throws -> Void
        public let values: PulledValues<BridgeSelectionSummary>

        public init(
            setSelected: @escaping @Sendable ([Data]) throws -> Void,
            values: PulledValues<BridgeSelectionSummary>
        ) {
            self.setSelected = setSelected
            self.values = values
        }

        init(_ subscription: ImportSelectionSubscription) {
            self.init(
                setSelected: { try subscription.setSelected(selected: $0) },
                values: PulledValues(subscription)
            )
        }
    }
#endif
