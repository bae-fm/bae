import BaeKit

/// Why a step the app runs before or around opening a library failed, and
/// which step it was: the welcome screen titles the failure by its step, and
/// the shell shows the error under its content.
struct LoadError: Equatable {
    enum Step: Equatable {
        /// Opening a library, or the work around opening one.
        case libraryOpen
        #if BAE_DEV_RESET
            /// The debug app's `--dev-reset` launch.
            case devReset
        #endif
    }

    let step: Step
    let error: DisplayError

    init(libraryOpen error: DisplayError) {
        self.step = .libraryOpen
        self.error = error
    }

    /// Fails when core says the error has nothing to show: a cancellation.
    init?(libraryOpen error: any Error) {
        guard let error = DisplayError(error) else { return nil }
        self.init(libraryOpen: error)
    }

    #if BAE_DEV_RESET
        /// Fails when core says the error has nothing to show: a
        /// cancellation.
        init?(devReset error: any Error) {
            guard let error = DisplayError(error) else { return nil }
            self.step = .devReset
            self.error = error
        }
    #endif
}
