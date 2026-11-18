import Combine
import Foundation

/// Requests to show something, such as an album in the album grid, as a
/// stream the one that shows it listens to. A request sent while nothing
/// listens waits for the next listener, as a navigation may switch to the
/// section that shows it before that section's view mounts; a later request
/// replaces it, and the next listener takes it once.
final class HeldRequests<Request>: @unchecked Sendable {
    private let listening: Listening<Request>

    /// Each listener gets the request waiting for one, if any, then the
    /// requests sent from then on, on the main queue's next turn, so a
    /// request never arrives while SwiftUI subscribes the listener.
    ///
    /// Built once, so every read of it is the same publisher. It holds the
    /// listeners' state and not this object, so nothing holds itself.
    let requests: AnyPublisher<Request, Never>

    init() {
        let listening = Listening<Request>()
        self.listening = listening
        requests = Deferred { listening.subscribe() }
            .receive(on: DispatchQueue.main)
            .eraseToAnyPublisher()
    }

    func send(_ request: Request) {
        listening.send(request)
    }
}

/// Who listens to held requests, and the request waiting for a listener.
/// Each subscription holds it for as long as it is subscribed.
private final class Listening<Request>: @unchecked Sendable {
    private let lock = NSLock()
    private let sent = PassthroughSubject<Request, Never>()
    /// The latest request sent while nothing listened.
    private var waiting: Request?
    private var listeners = 0

    func subscribe() -> AnyPublisher<Request, Never> {
        let waiting = lock.withLock {
            listeners += 1
            defer { self.waiting = nil }
            return self.waiting
        }
        return waiting.publisher
            .append(sent)
            .handleEvents(receiveCancel: { self.unsubscribe() })
            .eraseToAnyPublisher()
    }

    private func unsubscribe() {
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
