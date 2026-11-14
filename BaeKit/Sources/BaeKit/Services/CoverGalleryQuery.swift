import Foundation

/// A library release's cover picker as a live read: the release and the
/// source it shows change in place, and each value names the request it
/// answers.
public struct CoverGalleryQuery: Sendable {
    public let setRequest:
        @Sendable (_ releaseId: String, _ filter: BridgeCoverFilter) throws ->
            Void
    public let values: PulledValues<BridgeCoverGallerySnapshot>

    public init(
        setRequest:
            @escaping @Sendable (String, BridgeCoverFilter) throws -> Void,
        values: PulledValues<BridgeCoverGallerySnapshot>
    ) {
        self.setRequest = setRequest
        self.values = values
    }

    #if os(macOS)
        init(_ subscription: CoverGallerySubscription) {
            self.init(
                setRequest: {
                    try subscription.setRequest(releaseId: $0, filter: $1)
                },
                values: PulledValues(subscription)
            )
        }
    #endif
}
