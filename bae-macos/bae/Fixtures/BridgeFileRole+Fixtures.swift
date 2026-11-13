import BaeKit

@testable import bae

extension BridgeFileRole {
    var isAudio: Bool {
        if case .audio = self { return true }
        return false
    }
}
