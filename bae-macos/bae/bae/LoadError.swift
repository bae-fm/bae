import BaeKit

/// Why a step the app runs before or around opening a library failed, and
/// which step it was. Each step titles the failure by what failed and says
/// what the person can do next, wherever the failure shows: the welcome
/// screen, the shell's bottom line, or the reset screen.
struct LoadError: Equatable {
    enum Step: Equatable {
        /// Opening a library.
        case libraryOpen
        /// Opening a library found this Mac's copy unusable; the reset
        /// screen it lands on offers the way out.
        case resetOffered
        /// Reading which libraries are on this Mac.
        case libraryList
        /// Starting the keychain every library's keys live in.
        case secureStorage
        /// Reading the bundled Dropbox and OneDrive sign-in settings.
        case signInSettings
        /// Closing the open library to restore it over this Mac's copy.
        case restoreClose
        /// Resetting this Mac's copy of a library.
        case deviceReset
        /// Closing the open library.
        case shutdown
        /// Removing the open library from this Mac.
        case removal
        #if BAE_DEV_RESET
            /// The debug app's `--dev-reset` launch.
            case devReset
        #endif

        /// What failed.
        var title: String {
            switch self {
            case .libraryOpen:
                String(localized: "Library failed to open")
            case .resetOffered:
                localizedCoreString("ui.load_error.reset_offered.title")
            case .libraryList:
                localizedCoreString("ui.load_error.library_list.title")
            case .secureStorage:
                localizedCoreString("ui.load_error.secure_storage.title")
            case .signInSettings:
                localizedCoreString("ui.load_error.sign_in_settings.title")
            case .restoreClose:
                localizedCoreString("ui.load_error.restore_close.title")
            case .deviceReset:
                localizedCoreString("ui.load_error.device_reset.title")
            case .shutdown:
                localizedCoreString("ui.load_error.shutdown.title")
            case .removal:
                localizedCoreString("ui.load_error.removal.title")
            #if BAE_DEV_RESET
                case .devReset:
                    String(localized: "Dev reset failed")
            #endif
            }
        }

        /// What the person can do next. A reset-offered failure has none of
        /// its own: the reset screen it lands on is the way out.
        var advice: String? {
            switch self {
            case .libraryOpen:
                String(
                    localized:
                        "Choose another library, or restore it with “Add device”."
                )
            case .resetOffered:
                nil
            case .libraryList:
                localizedCoreString("ui.load_error.library_list.advice")
            case .secureStorage:
                localizedCoreString("ui.load_error.secure_storage.advice")
            case .signInSettings:
                localizedCoreString("ui.load_error.sign_in_settings.advice")
            case .restoreClose:
                localizedCoreString("ui.load_error.restore_close.advice")
            case .deviceReset:
                localizedCoreString("ui.load_error.device_reset.advice")
            case .shutdown:
                localizedCoreString("ui.load_error.shutdown.advice")
            case .removal:
                localizedCoreString("ui.load_error.removal.advice")
            #if BAE_DEV_RESET
                case .devReset:
                    String(
                        localized:
                            "Nothing after the failed step ran. Fix what the error names, then run scripts/reset-bae.py again."
                    )
            #endif
            }
        }
    }

    let step: Step
    let error: DisplayError

    init(_ step: Step, _ error: DisplayError) {
        self.step = step
        self.error = error
    }

    /// Fails when core says the error has nothing to show: a cancellation.
    init?(_ step: Step, _ error: any Error) {
        guard let error = DisplayError(error) else { return nil }
        self.init(step, error)
    }
}
