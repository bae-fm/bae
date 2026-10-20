import Foundation
import Network

/// Tells the open library when its network may be back, so sync work that
/// failed for want of a network runs now instead of sitting out a backoff of
/// up to five minutes.
///
/// It reports when the network path becomes satisfied, or stays satisfied
/// over a different preferred interface — the device moved to another
/// network. Updates about a path already reported, and losing it, report
/// nothing. The app also reports the app coming to the foreground and the
/// device waking (`AppService.reportNetworkChange`); this covers the network
/// itself.
///
/// It lives with one open session: started once when the session is wired
/// up, stopped when the library shuts down, and cancelled with its owner.
@MainActor
final class NetworkPathReports {
    private let monitor = NWPathMonitor()
    private var transitions = NetworkPathTransitions()
    private let report: @MainActor () -> Void

    init(report: @escaping @MainActor () -> Void) {
        self.report = report
    }

    /// Start watching. The monitor delivers the current path first, so a
    /// usable network already up reports once: a backoff saved before the
    /// library opened ends now.
    func start() {
        monitor.pathUpdateHandler = { [weak self] path in
            MainActor.assumeIsolated {
                self?.receive(path)
            }
        }
        monitor.start(queue: .main)
    }

    /// Stop watching for good; a cancelled monitor never restarts.
    func stop() {
        monitor.cancel()
    }

    private func receive(_ path: NWPath) {
        let changed = transitions.update(
            path.status == .satisfied
                ? .satisfied(interface: path.availableInterfaces.first?.name)
                : .unsatisfied
        )
        if changed {
            report()
        }
    }

    deinit {
        monitor.cancel()
    }
}

/// Whether a network path update is the network becoming usable: the first
/// satisfied path, or a satisfied path over a different preferred interface
/// than the one last reported. Anything else is not a change.
public struct NetworkPathTransitions: Sendable {
    public enum Path: Equatable, Sendable {
        case unsatisfied
        /// Satisfied over `interface`, the path's preferred interface, if it
        /// names one.
        case satisfied(interface: String?)
    }

    private var current: Path = .unsatisfied

    public init() {}

    /// Record `path`; true when it is a change worth reporting.
    public mutating func update(_ path: Path) -> Bool {
        defer { current = path }
        guard case .satisfied = path else { return false }
        return path != current
    }
}
