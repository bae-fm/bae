import BaeKit

extension BridgeServerError {
    var detail: String {
        switch self {
        case .invalidConfig(let detail),
            .credentialUnavailable(let detail),
            .bindFailed(let detail),
            .serverFailed(let detail):
            detail
        }
    }
}
