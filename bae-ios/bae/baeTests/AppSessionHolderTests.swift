import BaeKit
import Foundation
import Testing

@testable import bae

/// Covers the `AppSessionHolder` transitions that don't require a live library.
/// Opening a library goes through the bridge's `initApp` and produces a real
/// `AppService`, so opening, resetting and removing a library — and the
/// derived getters once one is open — need a real opened core and aren't
/// unit-tested here.
@MainActor
@Suite("AppSessionHolder", .timeLimit(.minutes(1)))
struct AppSessionHolderTests {
    /// A holder wired to a no-op telemetry sink and a host with nothing
    /// registered, over a throwaway directory.
    private func makeHolder() throws -> AppSessionHolder {
        let appDir = try BridgeAppDir(
            home: FileManager.default.temporaryDirectory
                .appendingPathComponent(UUID().uuidString)
                .path
        )
        let diagnostics = configureDiagnostics(
            config: .disabled,
            bugReporter: CoreBugReporter()
        )
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

    @Test("a fresh holder starts on the loading screen with no open library")
    func freshHolderIsLoading() throws {
        let holder = try makeHolder()

        guard case .loading = holder.screen else {
            Issue.record("A fresh holder should start on .loading")
            return
        }
        #expect(holder.appService == nil)
        #expect(holder.activeLibraryId == nil)
        #expect(!holder.hasMultipleLibraries)
    }

    @Test("no library is active before one is opened")
    func nothingActiveBeforeOpen() throws {
        let holder = try makeHolder()
        #expect(!holder.isActive(makeLibrary(id: "lib-1")))
    }

    @Test("a device with no libraries onboards")
    func noLibrariesOnboards() async throws {
        let holder = try makeHolder()

        holder.start()
        await Observed.until {
            if case .onboarding = holder.screen { return true }
            return false
        }
        #expect(holder.libraries.isEmpty)
    }

    /// With no library open, the reset warning came from opening a library
    /// that would only fail the same way again, so leaving it opens nothing.
    @Test("leaving the reset warning with no open library onboards")
    func leaveResetWithoutServiceOnboards() throws {
        let holder = try makeHolder()

        holder.leaveRecovery()

        guard case .onboarding = holder.screen else {
            Issue.record("Leaving with no open library should onboard")
            return
        }
    }
}
