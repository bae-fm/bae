import Foundation

/// Operations that run before a library is open: listing this device's
/// libraries and the restore codes coven saved in iCloud Keychain, creating a
/// library, and starting a restore or join. One injectable seam, so previews
/// never read or write real application data.
public final class LibrarySetup: Sendable, Observable {
    public let listLibraries: @Sendable () async throws -> [BridgeLibrary]
    /// Create a library named by core's default generator, returning its open
    /// store.
    public let createLibrary: @Sendable () async throws -> OpenedLibrary
    /// Restore codes coven saved in iCloud Keychain. Throws when the keychain
    /// refuses the lookup — locked, or the display asleep — which a screen
    /// listing them must not read as "no restore codes".
    public let savedRestoreCodes:
        @Sendable () async throws -> [BridgeSavedRestoreCode]
    /// One restore or join attempt.
    private let makeBootstrap: @MainActor @Sendable () -> LibraryBootstrap

    public init(
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
    public static func live(
        host: BridgeHost,
        deviceName: String
    ) -> LibrarySetup {
        LibrarySetup(
            listLibraries: { try await host.listLibraries() },
            createLibrary: { try await host.createLibrary(name: nil) },
            savedRestoreCodes: { try await host.savedRestoreCodes() },
            makeBootstrap: {
                LibraryBootstrap(host: host, deviceName: deviceName)
            }
        )
    }

    /// A restore or join screen's state, starting attempts through this seam.
    @MainActor
    public func bootstrapForm(
        entry: BootstrapCodeForm.Entry,
        onLibraryReady:
            @escaping @MainActor (OpenedLibrary, BridgeError?) -> Void
    ) -> BootstrapCodeForm {
        BootstrapCodeForm(
            entry: entry,
            makeBootstrap: makeBootstrap,
            onLibraryReady: onLibraryReady
        )
    }

    #if DEBUG
        /// Serves `libraries` and no saved codes; every other operation throws.
        /// A restore or join started from a preview fails at once.
        public static func stub(
            libraries: [BridgeLibrary] = []
        ) -> LibrarySetup {
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
                            appDir: previewAppDir()
                        ),
                        deviceName: "Preview Device"
                    )
                }
            )
        }

        /// A fresh bae directory under the temporary directory.
        private static func previewAppDir() -> BridgeAppDir {
            let home = FileManager.default.temporaryDirectory
                .appendingPathComponent("bae-preview-\(UUID().uuidString)")
                .path
            do {
                return try BridgeAppDir(home: home)
            }
            catch {
                preconditionFailure(
                    "The preview's bae directory could not be created: \(error)"
                )
            }
        }
    #endif
}
