import Combine
import Foundation

/// Requests to show something, such as an album in the album grid, as a
/// stream the one that shows it listens to. A request sent while nothing
/// listens waits for the next listener, as a navigation may switch to the
/// section that shows it before that section's view mounts; a later request
/// replaces it, and the next listener takes it once.
final class HeldRequests<Request>: @unchecked Sendable {
    private let lock = NSLock()
    private let sent = PassthroughSubject<Request, Never>()
    /// The latest request sent while nothing listened.
    private var waiting: Request?
    private var listeners = 0

    /// Each listener gets the request waiting for one, if any, then the
    /// requests sent from then on, on the main queue's next turn, so a
    /// request never arrives while SwiftUI subscribes the listener. A
    /// subscription may outlive the requests, as a browse session's does the
    /// UiStore a test makes for it, so it holds them weakly.
    private(set) lazy var requests: AnyPublisher<Request, Never> =
        Deferred { [weak self] () -> AnyPublisher<Request, Never> in
            guard let self else { return Empty().eraseToAnyPublisher() }
            let waiting = lock.withLock {
                listeners += 1
                defer { self.waiting = nil }
                return self.waiting
            }
            return waiting.publisher
                .append(sent)
                .handleEvents(receiveCancel: { [weak self] in
                    self?.stopListening()
                })
                .eraseToAnyPublisher()
        }
        .receive(on: DispatchQueue.main)
        .eraseToAnyPublisher()

    private func stopListening() {
        lock.withLock { listeners -= 1 }
    }

    func send(_ request: Request) {
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
