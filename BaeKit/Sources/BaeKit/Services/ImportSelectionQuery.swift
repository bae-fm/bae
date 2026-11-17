import Foundation

// The import list is a desktop surface: bae-bridge compiles the import list's
// types only for desktop targets, so the iOS bindings do not have them.
#if os(macOS)

    /// A live query that reads the summary of the import list's selected
    /// rows: what they hold and which actions they offer. The query reads
    /// every selected row, including rows the list does not show. The caller
    /// changes the selected keys on the same query, and each value
    /// summarizes the keys the caller set last.
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
