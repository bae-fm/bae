import BaeKit
import SwiftUI

/// What launch brought up: the process-lifetime host, or why launch stopped
/// before the app could run.
enum AppLaunch {
    case ready(BridgeHost)
    case failed(DisplayError)
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

/// An error that is the whole screen's content, with its details.
struct ScreenError: View {
    let error: DisplayError

    var body: some View {
        ErrorDetailDisclosure(error: error)
            .padding(ThemeSpace.page)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}
