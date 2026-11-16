import Foundation

/// Reads a library release's or a candidate's cover picker through one live
/// query. `setRequest` changes whose covers, which source and which focused
/// cover the query reads, and each value carries the request it answers.
public struct CoverGalleryQuery: Sendable {
    public let setRequest:
        @Sendable (
            _ owner: BridgeCoverGalleryOwner, _ filter: BridgeCoverFilter,
            _ focus: BridgeCoverKey?
        ) throws ->
            Void
    public let values: PulledValues<BridgeCoverGallerySnapshot>

    public init(
        setRequest:
            @escaping @Sendable (
                BridgeCoverGalleryOwner, BridgeCoverFilter, BridgeCoverKey?
            )
            throws -> Void,
        values: PulledValues<BridgeCoverGallerySnapshot>
    ) {
        self.setRequest = setRequest
        self.values = values
    }

    #if os(macOS)
        init(_ subscription: CoverGallerySubscription) {
            self.init(
                setRequest: {
                    try subscription.setRequest(
                        owner: $0,
                        filter: $1,
                        focus: $2
                    )
                },
                values: PulledValues(subscription)
            )
        }
    #endif
}
