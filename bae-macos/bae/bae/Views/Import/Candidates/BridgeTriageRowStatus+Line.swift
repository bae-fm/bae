import BaeKit

extension BridgeTriageRowStatus {
    var line: BridgeTriageStateLine? {
        guard case .line(let line) = self else { return nil }
        return line
    }
}
