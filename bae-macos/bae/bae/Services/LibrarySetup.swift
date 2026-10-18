import BaeKit
import Foundation

/// Operations the welcome flow runs before a library is open: listing this
/// Mac's libraries and the restore codes coven saved in iCloud Keychain,
/// creating a library, and starting a restore or join. One injectable seam, so
/// previews never read or write real application data.
final class LibrarySetup: Sendable, Observable {
    let listLibraries: @Sendable () async throws -> [BridgeLibrary]
    /// Create a library named by core's default generator, returning its open
    /// store.
    let createLibrary: @Sendable () async throws -> OpenedLibrary
    /// Restore codes coven saved in iCloud Keychain. Throws when the keychain
    /// refuses the lookup — locked, or the display asleep — which the welcome
    /// screen must not read as "no restore codes".
    let savedRestoreCodes: @Sendable () async throws -> [BridgeSavedRestoreCode]
    /// One restore or join attempt.
    let makeBootstrap: @MainActor @Sendable () -> LibraryBootstrap

    init(
        listLibraries: @escaping @Sendable () async throws -> [BridgeLibrary],
        createLibrary: @escaping @Sendable () async throws -> OpenedLibrary,
        savedRestoreCodes:
            @escaping @Sendable () async throws -> [BridgeSavedRestoreCode],
        makeBootstrap: @escaping @MainActor @Sendable () -> LibraryBootstrap
    ) {
        self.listLibraries = listLibraries
        self.createLibrary = createLibrary
        self.savedRestoreCodes = savedRestoreCodes
        self.makeBootstrap = makeBootstrap
    }

    /// The production wiring over the process's host.
    static func live(host: BridgeHost, deviceName: String) -> LibrarySetup {
        LibrarySetup(
            listLibraries: { try await host.listLibraries() },
            createLibrary: { try await host.createLibrary(name: nil) },
            savedRestoreCodes: { try await host.savedRestoreCodes() },
            makeBootstrap: {
                LibraryBootstrap(host: host, deviceName: deviceName)
            }
        )
    }

    #if DEBUG
        /// Serves `libraries` and no saved codes; every other operation throws.
        /// A restore or join started from a preview fails at once.
        static func stub(libraries: [BridgeLibrary] = []) -> LibrarySetup {
            LibrarySetup(
                listLibraries: { libraries },
                createLibrary: { throw StubError.notImplemented },
                savedRestoreCodes: { [] },
                makeBootstrap: {
                    LibraryBootstrap(
                        host: BaeHost.make(
                            diagnostics: configureDiagnostics(
                                config: .disabled
                            ),
                            appDir: BridgeAppDir(
                                home: FileManager.default.temporaryDirectory
                                    .appendingPathComponent(
                                        "bae-preview-\(UUID().uuidString)"
                                    )
                                    .path
                            )
                        ),
                        deviceName: "Preview Mac"
                    )
                }
            )
        }
    #endif
}
