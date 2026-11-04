import BaeKit

/// What launch brought up: the process-lifetime host, or the step that failed
/// and its error.
enum AppLaunch {
    /// A launch step that can fail.
    enum Step {
        /// Building bae's directory or the host.
        case startup

        /// The step as the launch screen and the log name it.
        var name: String {
            switch self {
            case .startup: "startup"
            }
        }
    }

    case ready(BridgeHost)
    case failed(Step, String)
}
