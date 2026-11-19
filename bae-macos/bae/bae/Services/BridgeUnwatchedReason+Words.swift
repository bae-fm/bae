import BaeKit

extension BridgeUnwatchedReason {
    /// What the alert and the list's hover text say of the watched folder
    /// named `folder`: where the system stopped bae from watching, and when
    /// bae reads new changes there.
    func words(folder: String) -> String {
        switch cause {
        case .limitReached:
            localizedCoreString(
                "ui.import.folder.not_watched.limit_reached",
                shownDirectory,
                folder
            )
        case .refused:
            localizedCoreString(
                "ui.import.folder.not_watched.refused",
                shownDirectory,
                folder,
                message
            )
        }
    }
}
