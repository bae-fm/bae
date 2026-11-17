import BaeKit
import Foundation

/// Reads the images the lightbox of a candidate's folder shows through one
/// live query. `setRequest` changes the candidate and the image shown, and
/// each value carries the request it answers.
struct FolderImagesQuery: Sendable {
    let setRequest: @Sendable (BridgeFolderImagesRequest?) throws -> Void
    let values: PulledValues<BridgeFolderImagesSnapshot>

    init(
        setRequest:
            @escaping @Sendable (BridgeFolderImagesRequest?) throws -> Void,
        values: PulledValues<BridgeFolderImagesSnapshot>
    ) {
        self.setRequest = setRequest
        self.values = values
    }

    init(_ subscription: FolderImagesSubscription) {
        self.init(
            setRequest: { try subscription.setRequest(request: $0) },
            values: PulledValues(subscription)
        )
    }
}
