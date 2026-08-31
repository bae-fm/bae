import BaeKit
import SwiftUI

/// A running import's line under a triage row: the phase it is in, how far
/// through it is, and the bar.
///
/// A leaf of its own because it is the only part of a row that changes by the
/// second. It subscribes to the candidate-runtime signal and filters to its
/// own key, so a progress tick redraws this and nothing else — the row itself
/// only says *that* an import is running, which is a fact the queue projects.
struct ImportProgressLine: View {
    let key: String

    var body: some View {
        CandidateRuntimeReader(key: key) { runtime in
            let percent = runtime?.import?.progressPercent
            VStack(alignment: .leading, spacing: 0) {
                Text(line(percent: percent, step: runtime?.import?.step))
                    .font(.system(size: 12.5))
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .truncationMode(.middle)
                    .padding(.top, 1)
                ProgressTrackBar(
                    progress: percent.map { Double($0) / 100 },
                    trackHeight: 3
                )
                .padding(.top, 7)
            }
        }
    }

    /// A row placed as importing whose run has not reported yet is at the
    /// start with no phase named.
    private func line(percent: UInt32?, step: BridgeImportStep?) -> String {
        let phaseText =
            step?.localizedText ?? String(localized: "Importing\u{2026}")
        // The percent renders through `.formatted(.percent)` before it's
        // interpolated, so the localized template only ever sees two string
        // slots — a literal `%` next to a format specifier is ambiguous to
        // hand-author as a catalog key.
        guard let percent else { return phaseText }
        let percentText = (Double(percent) / 100).formatted(.percent)
        return String(localized: "\(phaseText) \u{b7} \(percentText)")
    }
}
