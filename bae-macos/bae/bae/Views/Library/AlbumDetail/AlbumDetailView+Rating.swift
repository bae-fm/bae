import BaeKit

extension AlbumDetailView {
    /// Sets the person's rating on the album or a track to these stars, or
    /// clears it when stars is nil. When the write fails, the app shows the
    /// error.
    func rate(_ target: BridgeRatingTarget, _ stars: BridgeStars?) {
        let library = library
        let uiStore = uiStore
        // Nothing keeps or cancels this task: the library saves the
        // person's rating even when they leave the page right away.
        Task {
            do {
                try await library.setRating(target, stars)
            }
            catch {
                uiStore.showError(error)
            }
        }
    }
}
