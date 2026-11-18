import BaeKit
import Observation
import SwiftUI

/// What the grid asks core to place: the open album, in the group of the
/// artist it was opened under when the grid groups by artist, else at its
/// first card, under the grid's sort and grouping.
struct AlbumPlaceRequest: Equatable {
    let albumId: String
    let sectionId: String?
    let sortCriteria: [BridgeSortCriterion]
    let groupByArtist: Bool
}

/// Where core places the open album's card in the grid, read live: each
/// write that moves the card moves the place, so the grid draws the album's
/// detail under the row that holds the card.
@MainActor
@Observable
final class AlbumGridOpenPlace {
    /// The last place core answered, with the request it answers; nil before
    /// the first answer, and when core places no such card.
    private(set) var answer:
        (request: AlbumPlaceRequest, place: BridgeAlbumPlace)?

    /// Follow where core places `request`'s card until the calling task is
    /// cancelled. The place answered for the request before stays until the
    /// new request's first answer.
    func follow(
        _ request: AlbumPlaceRequest,
        in library: Library,
        showError: @escaping @MainActor (any Error) -> Void
    ) async {
        let pulls = ScreenSubscription(
            library.albumPlaceReads(
                request.sortCriteria,
                request.albumId,
                request.groupByArtist,
                request.sectionId
            ),
            onValue: { [weak self] place in
                guard let self else { return }
                let answer = place.map { (request: request, place: $0) }
                // The detail of an album that was not open opens as a click
                // opens it; a moved card moves the open detail at once.
                if self.answer?.request.albumId != request.albumId {
                    withAnimation(.spring(response: 0.3, dampingFraction: 0.85))
                    {
                        self.answer = answer
                    }
                }
                else {
                    self.answer = answer
                }
            },
            onError: { error in showError(error) }
        )
        await Task.untilCancelled()
        pulls.stop()
    }

    /// Forget the place: no album is open.
    func clear() {
        answer = nil
    }
}
