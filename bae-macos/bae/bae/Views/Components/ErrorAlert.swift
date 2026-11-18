import BaeKit
import SwiftUI

extension View {
    /// The shared "Error" alert driven by `uiStore.lastError`. Presents while an
    /// error is set, names the concrete fault under core's category line when
    /// core says the fault adds to it, offers a "Copy details" button for the
    /// whole chain, and clears the error on dismiss. Used by every scene that surfaces `UiStore` errors (the main
    /// window and the Storage Manager window own separate alerts on the same
    /// store).
    ///
    /// The fault is in the message, not only behind "Copy details": the category
    /// line alone ("Something went wrong.") names nothing, and an error a reader
    /// has to paste somewhere to identify has not been reported to them.
    func errorAlert(_ uiStore: UiStore) -> some View {
        errorAlert(
            "Error",
            error: Binding(
                get: { uiStore.lastError },
                set: { if $0 == nil { uiStore.clearError() } }
            )
        )
    }

    /// Shows an alert for why playback last failed while
    /// `PlaybackStore.failure` is set, on any screen. A later failure replaces
    /// the one shown. An AirPlay receiver that no longer accepts this device
    /// gets Could not cast with Pair again…, which calls `pairAgain` with the
    /// receiver; any other failure gets the shared Error alert. Each button
    /// calls `dismiss`, which asks core to clear the failure, and the alert
    /// closes when `PlaybackStore.failure` is `nil` again.
    func playbackFailureAlert(
        _ playbackStore: PlaybackStore,
        pairAgain: @escaping @MainActor (BridgeCastReceiver) -> Void,
        dismiss: @escaping @MainActor () -> Void
    ) -> some View {
        macAlert(
            item: Binding(
                get: {
                    playbackStore.failure.flatMap { failure in
                        DisplayError(failure).map { (failure, $0) }
                    }
                },
                set: { if $0 == nil { dismiss() } }
            )
        ) { shown in
            let (failure, error) = shown
            if case .audioOutput(.airPlayPairingLost(let receiver)) = failure {
                return CastAlertContent.pairingLost(error: error) {
                    pairAgain(receiver)
                }
            }
            return ErrorAlertContent.alert(title: Text("Error"), error: error)
        }
    }

    /// Shows why the receiver of the last cast refused it, titled with the
    /// receiver's name, until the person dismisses it or casts again.
    func castRefusalAlert(_ castStart: CastStart) -> some View {
        macAlert(
            item: Binding(
                get: { castStart.refusal },
                set: { if $0 == nil { castStart.dismissRefusal() } }
            )
        ) { refusal in
            CastAlertContent.refused(by: refusal.receiver, error: refusal.error)
        }
    }

    /// An alert titled `title` for whichever failure `error` holds, shaped like
    /// the shared one: core's line, the concrete fault under it, and "Copy
    /// details" for the whole chain. Dismissing it clears `error`.
    func errorAlert(
        _ title: LocalizedStringKey,
        error: Binding<DisplayError?>
    ) -> some View {
        macAlert(item: error) { error in
            ErrorAlertContent.alert(title: Text(title), error: error)
        }
    }
}

/// What the alert for a failure says: core's line, the concrete fault under it
/// when there is one, and OK with Copy details for the whole chain.
enum ErrorAlertContent {
    static func alert(
        title: Text,
        error: DisplayError
    ) -> (MacAlertWords, [MacAlertButton]) {
        var buttons = [MacAlertButton(Text("OK"))]
        if let detail = error.detail {
            buttons.append(
                MacAlertButton(Text("Copy details")) {
                    SystemActions.copyToPasteboard(detail)
                }
            )
        }
        // Verbatim: both halves are already resolved — core's localized line
        // and its untranslated fault — so this is a join, not prose for a
        // translator.
        let message =
            error.detailSummary.map { "\(error.line)\n\n\($0)" }
            ?? error.line
        return (
            MacAlertWords(title, message: Text(verbatim: message)),
            buttons
        )
    }
}

/// What the alerts for a cast that failed say.
enum CastAlertContent {
    /// A receiver refused the cast: titled with its name, over core's line.
    static func refused(
        by receiver: String,
        error: DisplayError
    ) -> (MacAlertWords, [MacAlertButton]) {
        ErrorAlertContent.alert(
            title: Text("Could not cast to \(receiver)"),
            error: error
        )
    }

    /// The AirPlay receiver playback was on no longer accepts this device:
    /// core's line, with Pair again…, which runs `pairAgain`, and Cancel.
    static func pairingLost(
        error: DisplayError,
        pairAgain: @escaping @MainActor () -> Void
    ) -> (MacAlertWords, [MacAlertButton]) {
        (
            MacAlertWords(
                Text("Could not cast"),
                message: Text(verbatim: error.line)
            ),
            [
                MacAlertButton(Text("Pair again…"), action: pairAgain),
                MacAlertButton(Text("Cancel"), role: .cancel),
            ]
        )
    }
}
