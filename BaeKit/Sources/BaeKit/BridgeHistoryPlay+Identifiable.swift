import Foundation

extension BridgeHistoryPlay: Identifiable {}

/// History's paginated list; each row draws the play History's query
/// returned at its position.
public typealias HistoryList = PaginatedList<BridgeHistoryPlay>
