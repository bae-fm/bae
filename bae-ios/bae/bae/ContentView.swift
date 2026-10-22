import BaeKit
import SwiftUI

/// What launch brought up: the process-lifetime host, or why launch stopped
/// before the app could run.
enum AppLaunch {
    case ready(BridgeHost)
    case failed(ScreenFailure)
}

/// App root: why launch stopped, or this device's app session.
struct ContentView: View {
    let launch: AppLaunch
    let diagnostics: BridgeDiagnostics
    let deviceName: String

    var body: some View {
        Group {
            switch launch {
            case .ready(let host):
                AppSessionView(
                    diagnostics: diagnostics,
                    host: host,
                    deviceName: deviceName
                )

            case .failed(let error):
                ScreenError(error: error)
            }
        }
        .background(Theme.background)
    }
}

/// A failure that is the whole screen's content: what failed, its details,
/// and what to do next.
struct ScreenError: View {
    let error: ScreenFailure

    var body: some View {
        ScreenFailureNotice(failure: error)
            .padding(ThemeSpace.page)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}
