import Combine
import Foundation

/// A request to show an album in the album grid, by a navigation such as Go
/// to Now Playing rather than a click on its card, and the track in it whose
/// row to show, if any.
struct AlbumReveal: Equatable, Sendable {
    let albumId: String
    /// The track whose row the grid scrolls into view and then flashes, or
    /// `nil` for the album alone.
    let trackId: String?
}

/// The album grid's reveal requests, as a stream the mounted grid listens
/// to. A request sent while no grid listens waits for the grid that mounts
/// next, as the library section and the albums mode may mount the grid only
/// after the navigation that sends it; a later request replaces it, and the
/// next listener takes it once.
final class AlbumRevealRequests: @unchecked Sendable {
    private let lock = NSLock()
    private let sent = PassthroughSubject<AlbumReveal, Never>()
    /// The latest request sent while no grid listened.
    private var waiting: AlbumReveal?
    private var listeners = 0

    /// Each listener gets the request waiting for one, if any, then the
    /// requests sent from then on, on the main queue's next turn, so a
    /// request never arrives while SwiftUI subscribes the listener.
    private(set) lazy var requests: AnyPublisher<AlbumReveal, Never> =
        Deferred { [unowned self] in
            let waiting = lock.withLock {
                listeners += 1
                defer { self.waiting = nil }
                return self.waiting
            }
            return waiting.publisher
                .append(sent)
                .handleEvents(receiveCancel: { [unowned self] in
                    lock.withLock { listeners -= 1 }
                })
        }
        .receive(on: DispatchQueue.main)
        .eraseToAnyPublisher()

    func send(_ request: AlbumReveal) {
        let listened = lock.withLock {
            if listeners == 0 {
                waiting = request
            }
            return listeners > 0
        }
        if listened {
            sent.send(request)
        }
    }
}
