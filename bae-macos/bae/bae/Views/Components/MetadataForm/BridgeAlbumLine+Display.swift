import BaeKit

extension BridgeAlbumLine {
    var title: String? {
        switch self {
        case .untitled: nil
        case .titleAlone(let title), .titleAndArtists(let title, _): title
        }
    }

    var artistLine: BridgeCreditLine? {
        switch self {
        case .titleAlone: nil
        case .untitled(let artistLine): artistLine
        case .titleAndArtists(_, let artistLine): artistLine
        }
    }
}
