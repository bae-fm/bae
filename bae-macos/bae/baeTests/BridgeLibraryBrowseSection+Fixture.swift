import BaeKit

extension BridgeLibraryBrowseSection {
    /// An artist's group of `albumCount` albums from `firstPosition`, with
    /// no picture and the album count line core delivers.
    static func fixture(
        _ id: String,
        title: String,
        firstPosition: UInt64,
        albumCount: UInt64
    ) -> BridgeLibraryBrowseSection {
        BridgeLibraryBrowseSection(
            id: id,
            title: title,
            image: nil,
            firstPosition: firstPosition,
            albumCount: albumCount,
            countLine: BridgeCountLabel(
                key: "core.library.album_count",
                count: albumCount
            )
        )
    }
}
