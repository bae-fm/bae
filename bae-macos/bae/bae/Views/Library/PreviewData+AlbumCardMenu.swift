#if DEBUG
    // What the album grid's card-menu previews read.
    extension PreviewData {
        /// The bulk-action menu a grid card presents, with no-op actions.
        static func albumCardMenu(targetCount: Int = 1) -> AlbumCardMenu {
            AlbumCardMenu(
                targetCount: targetCount,
                onPlay: {},
                onAddToQueue: {},
                onAddNext: {},
                onDelete: targetCount > 1 ? nil : {},
            )
        }
    }
#endif
