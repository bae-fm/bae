import Foundation

/// Operations that run before a library is open: listing this device's
/// libraries and the restore codes coven saved in iCloud Keychain, deleting
/// a saved code, creating a library, and starting a restore or join. One injectable seam, so previews
/// never read or write real application data.
public final class LibrarySetup: Sendable, Observable {
    public let listLibraries: @Sendable () async throws -> [BridgeLibrary]
    /// Create a library named by core's default generator, returning its open
    /// store, its step written into the setup's stream.
    public let createLibrary:
        @Sendable (BridgeLibrarySetup) async throws -> OpenedLibrary
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
    /// A new setup call's steps, read through the process's sink.
    private let makeSetupSteps: @Sendable () -> LibrarySetupSteps

    public init(
        listLibraries: @escaping @Sendable () async throws -> [BridgeLibrary],
        createLibrary:
            @escaping @Sendable (BridgeLibrarySetup) async throws ->
            OpenedLibrary,
        savedRestoreCodes:
            @escaping @Sendable () async throws -> [BridgeSavedRestoreEntry],
        forgetSavedRestoreCode:
            @escaping @Sendable (_ libraryId: String) async throws -> Void,
        unfinishedJoins:
            @escaping @Sendable () async throws -> [BridgeUnfinishedJoin],
        makeBootstrap: @escaping @MainActor @Sendable () -> LibraryBootstrap,
        makeSetupSteps: @escaping @Sendable () -> LibrarySetupSteps
    ) {
        self.listLibraries = listLibraries
        self.createLibrary = createLibrary
        self.savedRestoreCodes = savedRestoreCodes
        self.forgetSavedRestoreCode = forgetSavedRestoreCode
        self.unfinishedJoins = unfinishedJoins
        self.makeBootstrap = makeBootstrap
        self.makeSetupSteps = makeSetupSteps
    }

    /// The steps of a setup call about to start, for its view to draw.
    public func setupSteps() -> LibrarySetupSteps { makeSetupSteps() }

    /// The production wiring over the process's host and sink.
    public static func live(
        host: BridgeHost,
        diagnostics: BridgeDiagnostics,
        deviceName: String
    ) -> LibrarySetup {
        LibrarySetup(
            listLibraries: { try await host.listLibraries() },
            createLibrary: {
                try await host.createLibrary(name: nil, setup: $0)
            },
            savedRestoreCodes: { try await host.savedRestoreCodes() },
            forgetSavedRestoreCode: {
                try await host.forgetSavedRestoreCode(libraryId: $0)
            },
            unfinishedJoins: { try await host.unfinishedJoins() },
            makeBootstrap: {
                LibraryBootstrap(
                    host: host,
                    diagnostics: diagnostics,
                    deviceName: deviceName
                )
            },
            makeSetupSteps: { LibrarySetupSteps(diagnostics: diagnostics) }
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
                createLibrary: { _ in throw StubError.notImplemented },
                savedRestoreCodes: { savedCodes },
                forgetSavedRestoreCode: { _ in
                    throw StubError.notImplemented
                },
                unfinishedJoins: { [] },
                makeBootstrap: {
                    let diagnostics = BaeDiagnostics.disabled()
                    return LibraryBootstrap(
                        host: BaeHost.make(
                            diagnostics: diagnostics,
                            appDir: previewAppDir()
                        ),
                        diagnostics: diagnostics,
                        deviceName: "Preview Device"
                    )
                },
                makeSetupSteps: {
                    LibrarySetupSteps(diagnostics: BaeDiagnostics.disabled())
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
