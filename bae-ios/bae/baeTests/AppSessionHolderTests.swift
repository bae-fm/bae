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
            settings: .readable(storage: nil),
            isActive: false
        )
    }

    private func waitUntil(_ condition: @MainActor () -> Bool) async {
        for _ in 0..<10_000 {
            if condition() { return }
            await Task.yield()
        }
        Issue.record("condition never became true")
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

    @Test("a device with no libraries onboards")
    func noLibrariesOnboards() async {
        let holder = makeHolder()

        holder.start()
        await waitUntil {
            if case .onboarding = holder.screen { return true }
            return false
        }
        #expect(holder.libraries.isEmpty)
    }

    @Test("leaving the reset warning with no open library lists libraries again")
    func cancelResetWithoutServiceStartsOver() async {
        let holder = makeHolder()

        holder.cancelDeviceReset()
        await waitUntil {
            if case .onboarding = holder.screen { return true }
            return false
        }
    }
}
