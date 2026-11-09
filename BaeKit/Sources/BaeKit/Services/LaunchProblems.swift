import SwiftUI

/// What launch found wrong that no library opening or closing changes: the
/// bundled Dropbox and OneDrive sign-in settings couldn't be read. Recorded
/// once, when the app launches, and kept for the run, apart from any
/// library's own failures, so neither hides the other.
public struct LaunchProblems: Equatable, Sendable {
    /// Why this build can't sign in to Dropbox or OneDrive.
    public let signInSettings: DisplayError?

    public static let none = LaunchProblems(signInSettings: nil)

    public init(signInSettings: DisplayError?) {
        self.signInSettings = signInSettings
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
        HStack(alignment: .top, spacing: ThemeSpace.related) {
            StatusGlyph(.danger)
                .themeIcon(.medium)
            VStack(alignment: .leading, spacing: ThemeSpace.inline) {
                Text(verbatim: localizedCoreString(title))
                    .themeText(.heading)
                ErrorNotice(error: error, bare: true)
                Text(verbatim: localizedCoreString(advice))
                    .themeText(.body)
                    .foregroundStyle(NoticeText.secondary)
            }
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
