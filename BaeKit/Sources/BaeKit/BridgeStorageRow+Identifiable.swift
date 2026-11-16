import Foundation

extension BridgeStorageRow: Identifiable {
    public var id: String {
        release.id
    }
}

/// The storage manager's paginated list; each row draws the
/// `BridgeStorageRow` (a release and its album) the manager's query returned
/// at its position.
public typealias StorageList = PaginatedList<BridgeStorageRow>
