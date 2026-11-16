import Foundation

/// Reads a library release's cover picker through one live query.
/// `setRequest` changes the release and source the query reads, and each
/// value carries the request it answers.
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
