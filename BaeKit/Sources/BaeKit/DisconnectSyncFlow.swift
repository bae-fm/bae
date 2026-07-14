import Foundation
import Observation
import os.log

private let logger = Logger.bae("DisconnectSyncFlow")

/// Confirmation-and-execution model for disconnecting the cloud provider from
/// this device, shared by the macOS and iOS settings screens. Built with
/// injected closures so each platform supplies its live bridge/keychain calls,
/// its localized message text (the base sentence differs — iOS notes that
/// reconnecting needs another device because it has no provider-setup flow,
/// macOS omits that because it does), and its error-string templates (tests and
/// the shared package can't resolve an app catalog's localized strings, so the
/// app resolving them owns them). The at-risk sentence is owned the same way:
/// bae-core supplies the count, the app pluralizes it for its locale.
///
/// Two ordering invariants the flow enforces:
/// - the disconnect runs off the main actor, because bae-core joins the
///   sync-loop thread and can block for the remainder of an in-flight cycle;
/// - the restore code is deleted from the iCloud keychain only after a
///   successful disconnect — the code embeds the cloud-home connection that
///   disconnect invalidates, so a failed disconnect must leave it in place.
@MainActor
@Observable
public final class DisconnectSyncFlow {
    /// How many releases live only in the cloud, from bae-core. `0` means nothing
    /// is at risk.
    private let cloudOnlyReleaseCount: @Sendable () async throws -> UInt64
    /// Renders that count as the localized at-risk sentence, with the app's own
    /// plural rules (`core.sync.cloud_only_releases`). Only called for counts > 0.
    private let atRiskMessage: (UInt64) -> String
    /// Disconnects the provider. Synchronous and join-blocking, so callers run
    /// it off the main actor.
    private let disconnect: @Sendable () throws -> Void
    /// Removes this library's restore code from the iCloud keychain.
    private let deleteRestoreCode: () -> Void
    /// The platform's base confirmation sentence, resolved against the app's
    /// localized catalog.
    private let baseMessage: () -> String
    /// Builds the inline error shown when the at-risk check itself fails, from
    /// the error's description.
    private let warningCheckFailedMessage: (String) -> String
    /// Builds the inline error shown when the disconnect fails, from the error's
    /// description.
    private let disconnectFailedMessage: (String) -> String

    /// Whether the confirmation dialog is shown.
    public var showConfirm = false
    /// The at-risk sentence to append to the confirmation body, or `nil` when
    /// nothing is at risk (or the check failed).
    public var extraWarning: String?
    /// Inline error line for the Sync section, or `nil`.
    public var error: String?

    @ObservationIgnored
    private var warningTask: Task<Void, Never>?

    public init(
        cloudOnlyReleaseCount: @escaping @Sendable () async throws -> UInt64,
        atRiskMessage: @escaping (UInt64) -> String,
        disconnect: @escaping @Sendable () throws -> Void,
        deleteRestoreCode: @escaping () -> Void,
        baseMessage: @escaping () -> String,
        warningCheckFailedMessage: @escaping (String) -> String,
        disconnectFailedMessage: @escaping (String) -> String
    ) {
        self.cloudOnlyReleaseCount = cloudOnlyReleaseCount
        self.atRiskMessage = atRiskMessage
        self.disconnect = disconnect
        self.deleteRestoreCode = deleteRestoreCode
        self.baseMessage = baseMessage
        self.warningCheckFailedMessage = warningCheckFailedMessage
        self.disconnectFailedMessage = disconnectFailedMessage
    }

    /// Confirmation body: the platform's base sentence and — when releases live
    /// only in the cloud — the localized at-risk sentence appended to it.
    public var message: String {
        guard let extraWarning else { return baseMessage() }
        return "\(baseMessage()) \(extraWarning)"
    }

    /// Query the at-risk warning, then open the confirmation. If the query
    /// itself fails, surface the error inline (so the user sees the data-loss
    /// check didn't run) and still open the confirmation so they can proceed
    /// or cancel.
    public func promptDisconnect() {
        error = nil
        warningTask?.cancel()
        warningTask = Task {
            do {
                let count = try await cloudOnlyReleaseCount()
                extraWarning = count > 0 ? atRiskMessage(count) : nil
            }
            catch is CancellationError {
                return
            }
            catch {
                logger.error(
                    "Failed to compute disconnect warning: \(error.localizedDescription)"
                )
                // No line means nothing to say — a cancellation. Don't invent one.
                self.error = error.displayLine.map(warningCheckFailedMessage)
                extraWarning = nil
            }
            showConfirm = true
        }
    }

    /// Cancel an in-flight warning query when the view disappears.
    public func cancelWarningTask() {
        warningTask?.cancel()
    }

    /// Disconnect off the main actor, then — only on success — delete the
    /// restore code. On failure the code is left untouched and the error is
    /// shown inline.
    public func confirm() async {
        let disconnect = disconnect
        do {
            try await DetachedWork.run { try disconnect() }
            error = nil
            deleteRestoreCode()
        }
        catch {
            logger.error("Failed to disconnect: \(error.localizedDescription)")
            self.error = error.displayLine.map(disconnectFailedMessage)
        }
    }
}
