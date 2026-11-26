import BaeKit
import SwiftUI

@testable import bae

extension ShotStage {
    /// `view`, whose art comes from `images`, ready once the store has
    /// handed back every image asked of it, decoded or failed, and
    /// `isReady` holds. The store hands an image back a main-actor step
    /// before the view that asked shows it, and a view asks a step after it
    /// is first drawn; the tree's coming to rest spans both.
    init(
        _ view: some View,
        images: ImageStore,
        isReady: @escaping @MainActor () -> Bool = { true },
        isDrawn: @escaping @MainActor (NSView) -> Bool = { _ in true }
    ) {
        self.init(
            view,
            isReady: { images.unansweredImages == 0 && isReady() },
            isDrawn: isDrawn
        )
    }
}
