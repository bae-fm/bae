import BaeKit
import Testing

@testable import bae

/// Covers the `AppSessionHolder` transitions that don't require a live library.
/// Opening a library goes through the global `initApp` / `discoverLibraries`
/// bridge functions and produces a real `AppService`, so `start`, `openLibrary`,
/// `retryUnlock`, `onLinked`, and `forgetActiveLibrary` — and the derived
/// getters once a library is open — need a real opened core and aren't
/// unit-tested here.
@MainActor
@Suite("AppSessionHolder")
struct AppSessionHolderTests {
    /// A holder wired to a no-op telemetry sink — these transitions never emit,
    /// so the disabled sink (which cannot fail to construct) is enough.
    private func makeHolder() -> AppSessionHolder {
        AppSessionHolder(diagnostics: configureDiagnostics(config: .disabled))
    }

    private func makeLibrary(id: String, isActive: Bool = false) -> BridgeLibrary {
        BridgeLibrary(
            id: id,
            name: "Library \(id)",
            path: "/tmp/\(id)",
            cloudProvider: nil,
            isActive: isActive
        )
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

    @Test("cancelling the unlock gate with no open library falls to onboarding")
    func cancelUnlockWithoutServiceGoesToOnboarding() {
        let holder = makeHolder()

        holder.cancelUnlock()

        guard case .onboarding = holder.screen else {
            Issue.record("cancelUnlock with no open service should show onboarding")
            return
        }
    }
}
