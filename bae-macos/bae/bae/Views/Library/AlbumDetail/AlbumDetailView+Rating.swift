import BaeKit

extension AlbumDetailView {
    /// Give the album or a track these stars, or clear the person's rating
    /// with none; a failure says why.
    func rate(_ target: BridgeRatingTarget, _ stars: BridgeStars?) {
        let library = library
        let uiStore = uiStore
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
