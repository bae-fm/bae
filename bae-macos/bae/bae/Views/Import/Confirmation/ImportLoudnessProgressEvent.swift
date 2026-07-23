import Foundation

/// Loudness-measurement tick for an importing candidate. `key` routes it to that
/// candidate's confirm pane; `fraction` (0...1) drives the determinate bar as the
/// scan creeps through each track, and `tracksDone`/`tracksTotal` label "N / M".
struct ImportLoudnessProgressEvent {
    let key: String
    let tracksDone: UInt32
    let tracksTotal: UInt32
    let fraction: Double
}
