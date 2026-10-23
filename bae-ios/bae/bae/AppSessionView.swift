import BaeKit
import SwiftUI
import UIKit

/// This device's app session over the launched host: opens an existing
/// library, onboards, or offers a device reset.
struct AppSessionView: View {
    @State
    private var holder: AppSessionHolder
    @State
    private var started = false
    @Environment(\.scenePhase)
    private var scenePhase

    @MainActor
    init(
        diagnostics: BridgeDiagnostics,
        host: BridgeHost,
        deviceName: String
    ) {
        _holder = State(
            initialValue: AppSessionHolder(
                diagnostics: diagnostics,
                host: host,
                deviceName: deviceName
            )
        )
    }

    var body: some View {
        screen
            .environment(holder.removals)
            .task {
                guard !started else {
                    return
                }
                started = true
                holder.start()
                await holder.removals.loadUnfinished()
            }
            .onChange(of: scenePhase) { _, phase in
                // Every unlock a person is present for makes the scene active, so
                // this is the only retry trigger a refused keychain read needs.
                if phase == .active {
                    holder.retryOpenIfKeychainWasLocked(
                        trigger: "the scene becoming active"
                    )
                    // The network may be back after the app sat in the
                    // background or the device slept: sync's offline backoffs
                    // end now rather than run out.
                    holder.appService?.reportNetworkChange()
                }
                // The only playback save point on iOS, since core keeps running for
                // background audio; the background task keeps iOS from suspending
                // the process before the save returns.
                if phase == .background {
                    Task { [service = holder.appService] in
                        let task = BackgroundSaveTask(name: "SavePlaybackState")
                        defer { task.end() }
                        do {
                            try await service?.savePlaybackState()
                        }
                        catch {
                            service?.showError(error)
                        }
                    }
                }
            }
    }

    @ViewBuilder
    private var screen: some View {
        switch holder.screen {
        case .loading:
            ProgressView()
                .frame(maxWidth: .infinity, maxHeight: .infinity)

        case .onboarding:
            OnboardingView(
                setup: holder.setup,
                onLibraryReady: holder.openOpenedLibrary
            )

        case .resetRequired(let libraryId, let error):
            DeviceResetScreen(
                prompt: .offered,
                error: error,
                onReset: { holder.resetDevice(libraryId: libraryId) },
                onCancel: holder.leaveRecovery
            )

        case .resetUnfinished(let libraryId, let error):
            DeviceResetScreen(
                prompt: .unfinished,
                error: error,
                onReset: { holder.resetDevice(libraryId: libraryId) },
                onCancel: holder.leaveRecovery
            )

        case .restoreRequired(let libraryId):
            BootstrapCodeScreen(
                form: holder.setup.bootstrapForm(
                    entry: .restoreOver(libraryId: libraryId),
                    onLibraryReady: holder.openOpenedLibrary
                ),
                onBack: { holder.openLibrary(id: libraryId) }
            )
            .id(libraryId)

        case .restoreUnfinished(let libraryId):
            BootstrapCodeScreen(
                form: holder.setup.bootstrapForm(
                    entry: .finishRestore(libraryId: libraryId),
                    onLibraryReady: holder.openOpenedLibrary
                ),
                onBack: holder.leaveRecovery
            )
            .id(libraryId)

        case .library(let service):
            service.installEnvironment(
                LibraryView()
                    .environment(holder)
            )

        case .keychainLocked:
            // The open retries on scene activation; the button covers any
            // other change that unlocked the keychain.
            VStack(spacing: ThemeSpace.edge) {
                Text("Library Locked")
                    .themeText(.title)
                Text(BridgeErrorCategory.keyringLocked.localizedLine)
                    .themeText(.body)
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
                Button("Try again") {
                    holder.retryOpenIfKeychainWasLocked(
                        trigger: "the retry button"
                    )
                }
                .buttonStyle(PrimaryButtonStyle())
            }
            .padding()

        case .failed(let error):
            ScreenError(error: error)
        }
    }
}

/// Owns one UIKit background task and ends it exactly once, whether the save
/// finishes or iOS expires it, because ending it twice is an over-release.
@MainActor
private final class BackgroundSaveTask {
    private var id: UIBackgroundTaskIdentifier = .invalid

    init(name: String) {
        id = UIApplication.shared.beginBackgroundTask(withName: name) {
            [weak self] in
            MainActor.assumeIsolated { self?.end() }
        }
    }

    func end() {
        guard id != .invalid else { return }
        UIApplication.shared.endBackgroundTask(id)
        id = .invalid
    }
}
