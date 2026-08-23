import BaeKit
import Combine
import SwiftUI

// MARK: - Environment key

extension EnvironmentValues {
    /// What every candidate has in flight, as one signal. No store holds a
    /// copy: a view filters this to the key it draws, the way the loudness bar
    /// does, so an import's progress ticks redraw one leaf rather than every
    /// row in the list.
    @Entry
    var candidateRuntimePublisher:
        AnyPublisher<BridgeCandidateRuntimeChange, Never> =
            Empty()
            .eraseToAnyPublisher()
}

// MARK: - Reader

/// Draws `content` with what is in flight for one key, kept current from the
/// shared signal.
///
/// It subscribes first and reads the key's current value second, so a view
/// that appears partway through a run shows what is happening instead of
/// waiting for the next change. Both happen on the main actor in that order,
/// so the read cannot be undone by a change that was already on its way.
struct CandidateRuntimeReader<Content: View>: View {
    let key: String
    @ViewBuilder
    let content: (BridgeCandidateRuntimeSnapshot?) -> Content

    @Environment(\.candidateRuntimePublisher)
    private var publisher
    @Environment(Importer.self)
    private var importer: Importer?

    @State
    private var runtime: BridgeCandidateRuntimeSnapshot?

    var body: some View {
        content(runtime)
            .onReceive(publisher) { apply($0) }
            .task(id: key) {
                // No importer is a preview or a test with no bridge behind
                // it: there is nothing to read, so whatever the signal said
                // stands.
                guard let importer else { return }
                runtime = importer.candidateRuntime(key)
            }
    }

    private func apply(_ change: BridgeCandidateRuntimeChange) {
        switch change {
        case .updated(let changed, let value):
            guard changed == key else { return }
            runtime = value
        case .removed(let changed):
            guard changed == key else { return }
            runtime = nil
        case .reset(let runtimes):
            // Deliveries were dropped, so this is the whole of what is running:
            // a key it does not name has nothing running for it.
            runtime = runtimes.first { $0.key == key }?.runtime
        }
    }
}

// MARK: - The state a surface shows

/// The identify state a candidate shows: the run in flight while there is one,
/// else the state its stored verdict stands back up as.
func shownIdentifyState(
    resumed: IdentifyState,
    runtime: BridgeCandidateRuntimeSnapshot?
) -> IdentifyState {
    guard let runtime else { return resumed }
    let live = IdentifyState(bridge: runtime.identifyState)
    if case .idle = live {
        return resumed
    }
    return live
}
