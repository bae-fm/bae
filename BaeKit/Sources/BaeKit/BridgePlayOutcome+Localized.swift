import Foundation

extension BridgePlayOutcome {
    /// What a play shows, or `nil` when it played everything it named. The
    /// lines for what it left out take how many, formatted for the locale.
    public var shown: DisplayError? {
        guard let key = bridgePlayOutcomeKey(outcome: self) else {
            return nil
        }
        let line = localizedCoreString(key)
        switch self {
        case .playingWithoutReleases(let count):
            return DisplayError(
                line: String(
                    format: line,
                    locale: Locale.current,
                    count
                )
            )
        case .playingWithoutTracks(let count):
            return DisplayError(
                line: String(
                    format: line,
                    locale: Locale.current,
                    count
                )
            )
        case .playing, .nothingToPlay, .startMissing:
            return DisplayError(line: line)
        }
    }
}
