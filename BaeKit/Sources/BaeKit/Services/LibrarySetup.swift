import Foundation

/// Operations that run before a library is open: listing this device's
/// libraries and the restore codes coven saved in iCloud Keychain, deleting
/// a saved code, creating a library, and starting a restore or join. One injectable seam, so previews
/// never read or write real application data.
public final class LibrarySetup: Sendable, Observable {
    public let listLibraries: @Sendable () async throws -> [BridgeLibrary]
    /// Create a library named by core's default generator, returning its open
    /// store.
    public let createLibrary: @Sendable () async throws -> OpenedLibrary
    /// Restore codes coven saved in iCloud Keychain, including ones bae
    /// can't read. Throws when the keychain refuses the lookup — locked, or
    /// the display asleep — which a screen listing them must not read as "no
    /// restore codes".
    public let savedRestoreCodes:
        @Sendable () async throws -> [BridgeSavedRestoreEntry]
    /// Delete the library's saved restore code, readable or not, from iCloud
    /// Keychain on all the person's Apple devices. The library and its
    /// storage stay.
    public let forgetSavedRestoreCode:
        @Sendable (_ libraryId: String) async throws -> Void
    /// The joins this device has under way, to resume.
    public let unfinishedJoins:
        @Sendable () async throws -> [BridgeUnfinishedJoin]
    /// One restore or join attempt.
    private let makeBootstrap: @MainActor @Sendable () -> LibraryBootstrap

    public init(
        listLibraries: @escaping @Sendable () async throws -> [BridgeLibrary],
        createLibrary: @escaping @Sendable () async throws -> OpenedLibrary,
        savedRestoreCodes:
            @escaping @Sendable () async throws -> [BridgeSavedRestoreEntry],
        forgetSavedRestoreCode:
            @escaping @Sendable (_ libraryId: String) async throws -> Void,
        unfinishedJoins:
            @escaping @Sendable () async throws -> [BridgeUnfinishedJoin],
        makeBootstrap: @escaping @MainActor @Sendable () -> LibraryBootstrap
    ) {
        self.listLibraries = listLibraries
        self.createLibrary = createLibrary
        self.savedRestoreCodes = savedRestoreCodes
        self.forgetSavedRestoreCode = forgetSavedRestoreCode
        self.unfinishedJoins = unfinishedJoins
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
            forgetSavedRestoreCode: {
                try await host.forgetSavedRestoreCode(libraryId: $0)
            },
            unfinishedJoins: { try await host.unfinishedJoins() },
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
            decodeCode: { try decodeCodeInfo(code: $0) },
            makeBootstrap: makeBootstrap,
            unfinishedJoins: unfinishedJoins,
            onLibraryReady: onLibraryReady
        )
    }

    #if DEBUG
        /// Serves `libraries` and `savedCodes`; every other operation
        /// throws. A restore, join or delete started from a preview fails at
        /// once.
        public static func stub(
            libraries: [BridgeLibrary] = [],
            savedCodes: [BridgeSavedRestoreEntry] = []
        ) -> LibrarySetup {
            LibrarySetup(
                listLibraries: { libraries },
                createLibrary: { throw StubError.notImplemented },
                savedRestoreCodes: { savedCodes },
                forgetSavedRestoreCode: { _ in
                    throw StubError.notImplemented
                },
                unfinishedJoins: { [] },
                makeBootstrap: {
                    LibraryBootstrap(
                        host: BaeHost.make(
                            diagnostics: configureDiagnostics(
                                config: .disabled,
                                bugReporter: CoreBugReporter()
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
