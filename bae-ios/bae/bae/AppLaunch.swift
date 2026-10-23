import BaeKit

/// What launch brought up: the process-lifetime host, or the step that failed
/// and its error.
enum AppLaunch {
    /// A launch step that can fail.
    enum Step {
        /// Building bae's directory or the host.
        case startup
        /// Starting the keychain every library's keys live in.
        case secureStorage

        /// The step as the launch screen and the log name it.
        var name: String {
            switch self {
            case .startup: "startup"
            case .secureStorage: "secure storage"
            }
        }
    }

    case ready(BridgeHost)
    case failed(Step, String)
}
