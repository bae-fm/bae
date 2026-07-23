#if DEBUG
    import BaeKit
    import Foundation

    // Preview fixtures for the prop-driven queue views (rows and sections take
    // loaded `QueueItem`s directly, not a store). Derived from
    // `PreviewData.queueEntries` so their ids line up with the store-driven
    // queue previews.
    extension PreviewData {
        static let queueItems: [QueueItem] = queueEntries.map(
            QueueItem.init(bridge:)
        )
    }
#endif
