import Foundation

extension BridgeArtistSummary: Identifiable {
    public var id: String { artistId }
}

public typealias ArtistList = PaginatedList<BridgeArtistSummary>
