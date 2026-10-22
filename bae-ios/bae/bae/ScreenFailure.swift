import BaeKit
import SwiftUI

/// A failure a whole screen shows, or the reset screen shows under its
/// warning: which step failed, the error, and what to do next.
struct ScreenFailure {
    enum Step {
        /// Building bae's directory or the host at launch.
        case startup
        /// Starting the keychain every library's keys live in.
        case secureStorage
        /// Reading which libraries are on this device.
        case libraryList
        /// Opening a library.
        case libraryOpen
        /// Opening a library found this device's copy unusable; the reset
        /// screen it lands on offers the way out.
        case resetOffered
        /// Resetting this device's copy of a library.
        case deviceReset

        /// What failed.
        var title: String {
            switch self {
            case .startup: localizedCoreString("ui.load_error.startup.title")
            case .secureStorage:
                localizedCoreString("ui.load_error.secure_storage.title")
            case .libraryList:
                localizedCoreString("ui.load_error.library_list.title")
            case .libraryOpen:
                localizedCoreString("ui.load_error.library_open.title")
            case .resetOffered:
                localizedCoreString("ui.load_error.reset_offered.title")
            case .deviceReset:
                localizedCoreString("ui.load_error.device_reset.title")
            }
        }

        /// What the person can do next. A reset-offered failure has none of
        /// its own: the reset screen it lands on is the way out.
        var advice: String? {
            switch self {
            case .startup: localizedCoreString("ui.load_error.startup.advice")
            case .secureStorage:
                localizedCoreString("ui.load_error.secure_storage.advice")
            case .libraryList:
                localizedCoreString("ui.load_error.library_list.advice")
            case .libraryOpen:
                localizedCoreString("ui.load_error.library_open.advice")
            case .resetOffered: nil
            case .deviceReset:
                localizedCoreString("ui.load_error.device_reset.advice")
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

/// A failure's title, its error with details, and what to do next.
struct ScreenFailureNotice: View {
    let failure: ScreenFailure

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.inline) {
            Text(verbatim: failure.step.title)
                .themeText(.heading)
            ErrorDetailDisclosure(error: failure.error)
            if let advice = failure.step.advice {
                Text(verbatim: advice)
                    .themeText(.body)
                    .foregroundStyle(.secondary)
            }
        }
    }
}
