import BaeKit
import Foundation
import os.log

private let logger = Logger.bae("DisconnectSyncFlow")

/// Confirmation-and-execution model for disconnecting the cloud provider from
/// this device. Built with injected closures so the view supplies the live
/// bridge calls and tests supply stubs.
///
/// Two ordering invariants the flow enforces:
/// - the disconnect runs off the main actor, because bae-core joins the
///   sync-loop thread and can block for the remainder of an in-flight cycle;
/// - the restore code is deleted from the iCloud keychain only after a
///   successful disconnect — the code embeds the cloud-home connection that
///   disconnect invalidates, so a failed disconnect must leave it in place.
@MainActor
@Observable
final class DisconnectSyncFlow {
    /// bae-core's pre-formatted at-risk sentence when releases live only in the
    /// cloud, or `nil` when nothing is at risk.
    private let warningMessage: @Sendable () async throws -> String?
    /// Disconnects the provider. Synchronous and join-blocking, so callers run
    /// it off the main actor.
    private let disconnect: @Sendable () throws -> Void
    /// Removes this library's restore code from the iCloud keychain.
    private let deleteRestoreCode: () -> Void

    /// Whether the confirmation dialog is shown.
    var showConfirm = false
    /// bae-core's pre-formatted at-risk sentence to append to the confirmation
    /// body, or `nil` when nothing is at risk (or the check failed).
    var extraWarning: String?
    /// Inline error line for the Sync section, or `nil`.
    var error: String?

    @ObservationIgnored
    private var warningTask: Task<Void, Never>?

    init(
        warningMessage: @escaping @Sendable () async throws -> String?,
        disconnect: @escaping @Sendable () throws -> Void,
        deleteRestoreCode: @escaping () -> Void
    ) {
        self.warningMessage = warningMessage
        self.disconnect = disconnect
        self.deleteRestoreCode = deleteRestoreCode
    }

    /// Confirmation body: the base sentence, the note that reconnecting needs
    /// another device (iOS has no provider-setup flow), and — when releases
    /// live only in the cloud — bae-core's pre-formatted at-risk sentence
    /// appended verbatim.
    var message: String {
        var text = String(
            localized:
                "This will stop syncing and remove the cloud provider configuration."
        )
        text +=
            " "
            + String(
                localized:
                    "To sync this library again, pair this device from another device."
            )
        guard let extraWarning else { return text }
        return "\(text) \(extraWarning)"
    }

    /// Query the at-risk warning, then open the confirmation. If the query
    /// itself fails, surface the error inline (so the user sees the data-loss
    /// check didn't run) and still open the confirmation so they can proceed
    /// or cancel.
    func promptDisconnect() {
        error = nil
        warningTask?.cancel()
        warningTask = Task {
            do {
                extraWarning = try await warningMessage()
            }
            catch is CancellationError {
                return
            }
            catch {
                logger.error(
                    "Failed to compute disconnect warning: \(error.localizedDescription)"
                )
                self.error = String(
                    localized:
                        "Couldn't check for cloud-only releases: \(error.localizedDescription)"
                )
                extraWarning = nil
            }
            showConfirm = true
        }
    }

    /// Cancel an in-flight warning query when the view disappears.
    func cancelWarningTask() {
        warningTask?.cancel()
    }

    /// Disconnect off the main actor, then — only on success — delete the
    /// restore code. On failure the code is left untouched and the error is
    /// shown inline.
    func confirm() async {
        let disconnect = disconnect
        do {
            try await DetachedWork.run { try disconnect() }
            error = nil
            deleteRestoreCode()
        }
        catch {
            logger.error("Failed to disconnect: \(error.localizedDescription)")
            self.error = String(
                localized: "Failed to disconnect: \(error.localizedDescription)"
            )
        }
    }
}
