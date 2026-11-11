import Foundation

/// Which of the app's own problems the Activity panel lists, by kind rather
/// than by words: a sync failure whose text changes is still the same
/// problem, so a dismissal of it stands.
public enum ActivityProblemID: Hashable, Sendable {
    /// The bundled sign-in settings couldn't be read.
    case signInSettings
    /// A library failed to open, close or be removed while another stays
    /// open.
    case libraryLoad
    /// The Activity panel couldn't read sync's state.
    case syncStatusUnread
    /// Sync's own failure.
    case syncFailure
    /// The Subsonic server is switched on and not running.
    case subsonicNotRunning
}

/// A problem the app found in itself, as the Activity panel lists it: what
/// failed, what to do next, and the error behind its details.
public struct AppProblem: Equatable, Sendable, Identifiable {
    public let id: ActivityProblemID
    public let title: String
    public let advice: String?
    public let error: DisplayError

    public init(
        id: ActivityProblemID,
        title: String,
        advice: String?,
        error: DisplayError
    ) {
        self.id = id
        self.title = title
        self.advice = advice
        self.error = error
    }
}
