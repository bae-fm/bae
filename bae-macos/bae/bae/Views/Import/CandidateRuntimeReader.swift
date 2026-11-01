import BaeKit
import SwiftUI

// MARK: - Environment key

extension EnvironmentValues {
    /// What every candidate has in flight, by key, for the views drawing one
    /// key; `nil` where no import list is.
    @Entry
    var candidateRuntimes: CandidateRuntimes?
}

// MARK: - Reader

/// Draws `content` with what is in flight for one key: a run's identify state
/// and the signals it last read, a running import's step, which change as
/// they step. It redraws as that key changes or as keys come and go; how far
/// an import has got is not here but in the import percents' stream, which
/// only a native bar reads.
struct CandidateRuntimeReader<Content: View>: View {
    let key: Data
    @ViewBuilder
    let content: (BridgeCandidateRuntimeSnapshot?) -> Content

    @Environment(\.candidateRuntimes)
    private var runtimes

    var body: some View {
        content(runtimes?.runtime(forKey: key))
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

#if DEBUG
    extension View {
        /// What the reader above takes its values from: the runtimes
        /// `runtimes` holds, and the importer the views under it act
        /// through. A preview that draws a candidate — a row with a running
        /// import, a pane, the release editor's search — mounts it, so it
        /// provides them and renders the same chain the app does.
        func candidateReaderPreviewEnvironment(
            runtimes: CandidateRuntimes = CandidateRuntimes()
        ) -> some View {
            self
                .environment(Importer.stub())
                .environment(\.candidateRuntimes, runtimes)
        }
    }
#endif
