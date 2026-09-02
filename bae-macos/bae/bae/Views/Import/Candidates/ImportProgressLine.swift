import BaeKit
import SwiftUI

/// A running import's line under a triage row: the phase it is in and the
/// bar. The bar is how far through it is; a number beside it would say the
/// same thing again in a row that has no room for it.
///
/// A leaf of its own because it is the only part of a row that changes by the
/// second. It subscribes to the candidate-runtime signal and filters to its
/// own key, so a progress tick redraws this and nothing else — the row itself
/// only says *that* an import is running, which is a fact the queue projects.
struct ImportProgressLine: View {
    let key: String

    var body: some View {
        CandidateRuntimeReader(key: key) { runtime in
            ProgressLine(
                phaseText(step: runtime?.import?.step),
                progress: runtime?.import?.progressPercent
                    .map {
                        Double($0) / 100
                    }
            )
        }
    }

    /// A row placed as importing whose run has not reported yet is at the
    /// start with no phase named.
    private func phaseText(step: BridgeImportStep?) -> String {
        step?.localizedText ?? String(localized: "Importing\u{2026}")
    }
}
