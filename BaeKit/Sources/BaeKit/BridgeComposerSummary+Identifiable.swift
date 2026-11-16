import Foundation

extension BridgeComposerSummary: Identifiable {
    public var id: String { artistId }
}

public typealias ComposerList = PaginatedList<BridgeComposerSummary>
