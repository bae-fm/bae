import SwiftUI

/// What launch found wrong that no library opening or closing changes: the
/// bundled Dropbox and OneDrive sign-in settings couldn't be read. Recorded
/// once, when the app launches, and kept for the run, apart from any
/// library's own failures, so neither hides the other.
public struct LaunchProblems: Equatable, Sendable {
    /// Why this build can't sign in to Dropbox or OneDrive.
    public let signInSettings: DisplayError?

    public static let none = LaunchProblems(signInSettings: nil)

    /// Each problem, as the Activity panel and the welcome list it.
    public var problems: [AppProblem] {
        guard let signInSettings else { return [] }
        return [
            AppProblem(
                id: .signInSettings,
                title: localizedCoreString(
                    "ui.load_error.sign_in_settings.title"
                ),
                advice: localizedCoreString(
                    "ui.load_error.sign_in_settings.advice"
                ),
                error: signInSettings
            )
        ]
    }

    public init(signInSettings: DisplayError?) {
        self.signInSettings = signInSettings
    }
}

/// Each launch problem as a danger notice: what failed and what to do next,
/// with the error behind its Details.
public struct LaunchProblemNotices: View {
    private let problems: LaunchProblems

    public init(_ problems: LaunchProblems) {
        self.problems = problems
    }

    public var body: some View {
        ForEach(problems.problems) { problem in
            Notice(
                title: problem.title,
                advice: problem.advice,
                error: problem.error
            )
        }
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
