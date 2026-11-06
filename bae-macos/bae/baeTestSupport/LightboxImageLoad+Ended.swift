import BaeKit

@testable import bae

extension LightboxImageLoad {
    /// Whether the lightbox's load of `image`, or with none named of the
    /// image it opened on, has ended: decoded and read for Live Text, or
    /// failed. Waited for under observation, as `phase` changes.
    func ended(loading image: ImageContent? = nil) -> Bool {
        guard let content, image == nil || image == content else {
            return false
        }
        switch phase {
        case .analysed, .failed: return true
        case .loading, .decoded: return false
        }
    }
}
