import BaeKit
import Foundation
import Observation
import Testing

@testable import bae

/// Covers the `AppSessionHolder` transitions that don't require a live library.
/// Opening a library goes through the bridge's `initApp` and produces a real
/// `AppService`, so opening, resetting and removing a library — and the
/// derived getters once one is open — need a real opened core and aren't
/// unit-tested here.
@MainActor
@Suite("AppSessionHolder")
struct AppSessionHolderTests {
    /// A holder wired to a no-op telemetry sink and a host with nothing
    /// registered, over a throwaway directory.
    private func makeHolder() -> AppSessionHolder {
        let appDir = BridgeAppDir(
            home: FileManager.default.temporaryDirectory
                .appendingPathComponent(UUID().uuidString)
                .path
        )
        let diagnostics = configureDiagnostics(config: .disabled)
        return AppSessionHolder(
            diagnostics: diagnostics,
            host: BaeHost.make(diagnostics: diagnostics, appDir: appDir),
            deviceName: "Test Device"
        )
    }

    private func makeLibrary(id: String) -> BridgeLibrary {
        BridgeLibrary(
            id: id,
            name: "Library \(id)",
            path: "/tmp/\(id)",
            state: .openable(storage: nil),
            isActive: false
        )
    }

    /// Return once `holder.screen` satisfies `condition`, checking it again
    /// each time the screen changes: the wait ends on the change itself,
    /// however long the work behind it runs. A screen that never arrives
    /// holds the test until its time limit fails it.
    private func screen(
        of holder: AppSessionHolder,
        becomes condition: (AppScreen) -> Bool
    ) async {
        while !condition(holder.screen) {
            await withCheckedContinuation { continuation in
                withObservationTracking {
                    _ = holder.screen
                } onChange: {
                    continuation.resume()
                }
            }
        }
    }

    @Test("a fresh holder starts on the loading screen with no open library")
    func freshHolderIsLoading() {
        let holder = makeHolder()

        guard case .loading = holder.screen else {
            Issue.record("A fresh holder should start on .loading")
            return
        }
        #expect(holder.appService == nil)
        #expect(holder.activeLibraryId == nil)
        #expect(!holder.hasMultipleLibraries)
    }

    @Test("no library is active before one is opened")
    func nothingActiveBeforeOpen() {
        let holder = makeHolder()
        #expect(!holder.isActive(makeLibrary(id: "lib-1")))
    }

    @Test("a device with no libraries onboards", .timeLimit(.minutes(1)))
    func noLibrariesOnboards() async {
        let holder = makeHolder()

        holder.start()
        await screen(of: holder) {
            if case .onboarding = $0 { return true }
            return false
        }
        #expect(holder.libraries.isEmpty)
    }

    /// With no library open, the reset warning came from opening a library
    /// that would only fail the same way again, so leaving it opens nothing.
    @Test("leaving the reset warning with no open library onboards")
    func leaveResetWithoutServiceOnboards() {
        let holder = makeHolder()

        holder.leaveRecovery()

        guard case .onboarding = holder.screen else {
            Issue.record("Leaving with no open library should onboard")
            return
        }
    }
}
