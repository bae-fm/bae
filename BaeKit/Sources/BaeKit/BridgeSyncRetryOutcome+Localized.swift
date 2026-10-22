import Foundation

extension BridgeSyncRetryOutcome {
    /// What a retry that didn't sync shows, or `nil` when it synced. A failed
    /// pass shows its error; the provider's cooldown and sync not running
    /// resolve the line core keys for them, the cooldown's end formatted for
    /// the locale.
    public var failure: DisplayError? {
        if case .failed(let error) = self {
            return DisplayError(error)
        }
        guard let key = bridgeSyncRetryOutcomeKey(outcome: self) else {
            return nil
        }
        let line = localizedCoreString(key)
        guard case .coolingDown(let until) = self else {
            return DisplayError(line: line)
        }
        return DisplayError(
            line: String(
                format: line,
                locale: Locale.current,
                until.formatted(date: .abbreviated, time: .shortened)
            )
        )
    }
}
