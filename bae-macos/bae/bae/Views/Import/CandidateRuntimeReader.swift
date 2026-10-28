import BaeKit
import Combine
import SwiftUI

// MARK: - Environment key

extension EnvironmentValues {
    /// What every candidate has in flight, by key, for the views drawing one
    /// key; `nil` where no import list is.
    @Entry
    var candidateRuntimes: CandidateRuntimes?

    /// Each candidate's extracted signals as extraction settles them, on the
    /// same terms: one signal, filtered to the key that reads it.
    @Entry
    var candidateSignalsPublisher: AnyPublisher<CandidateSignalsEvent, Never> =
        Empty()
        .eraseToAnyPublisher()
}

/// One candidate's extracted signals, as extraction settles each of them.
/// `key` routes it to the form reading that candidate.
struct CandidateSignalsEvent {
    let key: String
    let signals: Signals
}

// MARK: - Reader

/// Draws `content` with what is in flight for one key: a run's identify state
/// and a running import's step, which change as they step. It redraws as
/// that key changes or as keys come and go; how far an import has got is not
/// here but in the import percents' stream, which only a native bar reads.
struct CandidateRuntimeReader<Content: View>: View {
    let key: String
    @ViewBuilder
    let content: (BridgeCandidateRuntimeSnapshot?) -> Content

    @Environment(\.candidateRuntimes)
    private var runtimes

    var body: some View {
        content(runtimes?.runtime(forKey: key))
    }
}

/// Draws `content` with the signals extraction has found for one key, kept
/// current from the shared signal.
///
/// Subscribes first and reads second, so a form opened partway through a run
/// starts with the pool the run has built rather than filling in from empty.
struct CandidateSignalsReader<Content: View>: View {
    let key: String
    @ViewBuilder
    let content: (Signals?) -> Content

    @Environment(\.candidateSignalsPublisher)
    private var publisher
    @Environment(Importer.self)
    private var importer: Importer?

    @State
    private var signals: Signals?

    var body: some View {
        content(signals)
            .onReceive(publisher) { event in
                guard event.key == key else { return }
                signals = event.signals
            }
            .task(id: key) {
                // No importer is a preview or a test with no bridge behind it:
                // there is nothing to read, so whatever the signal said stands.
                guard let importer else { return }
                if let read = importer.candidateSignals(key) {
                    signals = read
                }
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

#if DEBUG
    extension View {
        /// Everything the two readers above take their values from: the
        /// runtimes `runtimes` holds, the shared signals stream, and the
        /// importer the signals reader reads the current value off when it
        /// appears. A preview that draws a candidate — a row with a running
        /// import, a pane, the release editor's search — mounts them, so it
        /// provides them and renders the same chain the app does.
        func candidateReaderPreviewEnvironment(
            runtimes: CandidateRuntimes = CandidateRuntimes()
        ) -> some View {
            self
                .environment(Importer.stub())
                .environment(\.candidateRuntimes, runtimes)
                .environment(
                    \.candidateSignalsPublisher,
                    Empty<CandidateSignalsEvent, Never>().eraseToAnyPublisher()
                )
        }
    }
#endif
