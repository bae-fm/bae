import BaeKit
import SwiftUI

// MARK: - Environment key

extension EnvironmentValues {
    /// What each candidate has in flight, by key, for a shot or a test
    /// with no bridge behind it: a reader under it draws its key's entry and
    /// subscribes to nothing. `nil` in the app.
    @Entry
    var heldCandidateRuntimes: [Data: BridgeCandidateRuntimeSnapshot]?
}

// MARK: - Reader

/// Draws `content` with what is in flight for one key: a run's identify state
/// and the signals it last read, a running import's step, its typed search.
/// It pulls the key's own subscription, so it redraws only as that key
/// changes; how far an import has got is not here but in the import
/// percents' stream, which only a native bar reads.
struct CandidateRuntimeReader<Content: View>: View {
    let key: Data
    @ViewBuilder
    let content: (BridgeCandidateRuntimeSnapshot?) -> Content

    @Environment(\.heldCandidateRuntimes)
    private var held
    @Environment(Importer.self)
    private var importer: Importer?
    @Environment(UiStore.self)
    private var uiStore: UiStore?

    @State
    private var runtime: BridgeCandidateRuntimeSnapshot?
    @State
    private var subscription: ScreenSubscription<CandidateRuntimeSubscription>?

    var body: some View {
        content(held.map { $0[key] } ?? runtime)
            .task(id: key) {
                runtime = nil
                subscription = nil
                guard held == nil,
                    let source = importer?.subscribeCandidateRuntime(key)
                else { return }
                subscription = ScreenSubscription(
                    source,
                    onValue: { runtime = $0 },
                    onError: { error in uiStore?.showError(error) }
                )
            }
    }
}

// MARK: - The state a surface shows

/// The identify state a candidate shows: what identification is doing for it
/// while core says it is doing something, else the state its stored verdict
/// stands back up as.
func shownIdentifyState(
    resumed: IdentifyState,
    runtime: BridgeCandidateRuntimeSnapshot?
) -> IdentifyState {
    switch runtime?.identification {
    case nil: resumed
    case .queued: .queued
    case .run(let state): IdentifyState(bridge: state)
    }
}
