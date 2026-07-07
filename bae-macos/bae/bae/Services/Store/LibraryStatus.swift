import BaeKit
import Foundation

struct LibraryStatus: Equatable {
    let releaseInLibrary: Bool
    let albumInLibrary: Bool
    let albumId: String?

    init(bridge: BridgeLibraryStatus) {
        releaseInLibrary = bridge.releaseInLibrary
        albumInLibrary = bridge.albumInLibrary
        albumId = bridge.albumId
    }
}
