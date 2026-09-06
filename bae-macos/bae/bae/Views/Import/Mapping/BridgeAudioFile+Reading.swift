import BaeKit

extension BridgeAudioFile {
    var fileId: String {
        switch self {
        case .standalone(let fileId): fileId
        case .sheetSlice(let fileId, _, _): fileId
        }
    }
}
