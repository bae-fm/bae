import BaeKit
import SwiftUI

/// What launch brought up: the process-lifetime host, or why launch stopped
/// before the app could run.
enum AppLaunch {
    case ready(BridgeHost)
    case failed(String)
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

            case .failed(let message):
                ScreenError(message: message)
            }
        }
        .background(Theme.background)
    }
}

/// An error that is the whole screen's content.
struct ScreenError: View {
    let message: String

    var body: some View {
        ErrorText(message)
            .padding(ThemeSpace.page)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}
