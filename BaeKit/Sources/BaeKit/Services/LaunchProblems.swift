import SwiftUI

/// What launch found wrong that no library opening or closing changes: the
/// bundled Dropbox and OneDrive sign-in settings couldn't be read, or secure
/// storage didn't start. Recorded once, when the app launches, and kept for
/// the run, apart from any library's own failures, so neither hides the
/// other.
public struct LaunchProblems: Equatable, Sendable {
    /// Why this build can't sign in to Dropbox or OneDrive.
    public let signInSettings: DisplayError?
    /// Why no library can open: the keychain didn't start.
    public let secureStorage: DisplayError?

    public static let none = LaunchProblems(
        signInSettings: nil,
        secureStorage: nil
    )

    public init(signInSettings: DisplayError?, secureStorage: DisplayError?) {
        self.signInSettings = signInSettings
        self.secureStorage = secureStorage
    }
}

/// Each launch problem as a danger notice: what failed, the error with its
/// details, and what to do next.
public struct LaunchProblemNotices: View {
    private let problems: LaunchProblems

    public init(_ problems: LaunchProblems) {
        self.problems = problems
    }

    public var body: some View {
        if let error = problems.secureStorage {
            LaunchProblemNotice(
                title: "ui.load_error.secure_storage.title",
                advice: "ui.load_error.secure_storage.advice",
                error: error
            )
        }
        if let error = problems.signInSettings {
            LaunchProblemNotice(
                title: "ui.load_error.sign_in_settings.title",
                advice: "ui.load_error.sign_in_settings.advice",
                error: error
            )
        }
    }
}

private struct LaunchProblemNotice: View {
    let title: String
    let advice: String
    let error: DisplayError

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.inline) {
            Text(verbatim: localizedCoreString(title))
                .themeText(.heading)
            ErrorDetailDisclosure(error: error, showIcon: false)
            Text(verbatim: localizedCoreString(advice))
                .themeText(.body)
                .foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .notice(.danger)
    }
}

private struct LaunchProblemsKey: EnvironmentKey {
    static let defaultValue = LaunchProblems.none
}

extension EnvironmentValues {
    /// What launch found wrong, for every screen of the run to show.
    public var launchProblems: LaunchProblems {
        get { self[LaunchProblemsKey.self] }
        set { self[LaunchProblemsKey.self] = newValue }
    }
}
