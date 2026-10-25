#if BAE_DEV_RESET
    import BaeKit
    import Foundation
    import Synchronization
    import Testing

    @testable import bae

    /// What a dev reset asks of the bridge and the disk, in order.
    private final class ResetCalls: Sendable {
        private let recorded = Mutex<[String]>([])

        func record(_ call: String) {
            recorded.withLock { $0.append(call) }
        }

        var calls: [String] {
            recorded.withLock { $0 }
        }
    }

    private struct ProfileUnreadable: Error {}

    private let profile = BridgeDevResetProfile(
        libraryName: "dev",
        syncConfig: BridgeSaveSyncConfig(
            bucket: "bucket-name",
            region: "region-1",
            endpoint: "https://storage.example.test",
            keyPrefix: "prefix",
            accessKey: "access-key-id",
            secretKey: "secret-access-key",
            profile: nil
        ),
        discogsToken: "discogs-token",
        identifyAutomatically: false,
        importWhenIdentified: false
    )

    private func library(_ id: String, _ state: BridgeLibraryState)
        -> BridgeLibrary
    {
        BridgeLibrary(
            id: id,
            name: "Library",
            path: "/libraries/\(id)",
            state: state,
            isActive: false
        )
    }

    private let libraries = [
        library("library-1", .openable(storage: nil)),
        library("library-2", .replacementUnfinished(by: .restore)),
    ]

    private func reset(
        _ calls: ResetCalls,
        readProfile:
            @escaping @Sendable (String) throws ->
            BridgeDevResetProfile = { _ in profile },
        removal: @escaping @Sendable (BridgeLibrary) -> BridgeLibraryRemoval = {
            _ in .removed
        }
    ) -> DevReset {
        DevReset(
            readProfile: readProfile,
            finishRemovals: { calls.record("finish removals") },
            listLibraries: {
                calls.record("list libraries")
                return libraries
            },
            removeLibrary: { library in
                calls.record("remove \(library.id)")
                return removal(library)
            },
            savedRestoreCodes: {
                calls.record("saved restore codes")
                return [
                    .unreadable(libraryId: "library-2", reason: .format),
                    .unreadable(libraryId: "elsewhere", reason: .format),
                ]
            },
            forgetSavedRestoreCode: {
                calls.record("forget restore code \($0)")
            },
            replaceAppDirectory: { calls.record("replace app directory") },
            createLibrary: {
                calls.record("create \($0.libraryName)")
                return "new-library"
            }
        )
    }

    @Suite("DevReset")
    struct DevResetTests {
        /// Every library goes before bae's directory goes, only those
        /// libraries' saved restore codes are deleted, and the profile's
        /// library is created last.
        @Test(
            "removes every library, replaces bae's directory, then creates the profile's library"
        )
        func fullSequence() async throws {
            let calls = ResetCalls()

            let created = try await reset(calls)
                .run(
                    profilePath: "/profile.toml"
                )

            #expect(created == "new-library")
            #expect(
                calls.calls == [
                    "finish removals",
                    "list libraries",
                    "remove library-1",
                    "remove library-2",
                    "saved restore codes",
                    "forget restore code library-2",
                    "replace app directory",
                    "create dev",
                ]
            )
        }

        @Test(
            "a removal that didn't finish stops the reset before anything else goes"
        )
        func unfinishedRemovalStops() async {
            let calls = ResetCalls()
            let devReset = reset(
                calls,
                removal: { library in
                    library.id == "library-1"
                        ? .unfinished(
                            removal: BridgeUnfinishedRemoval(
                                error: .Diagnostic(
                                    category: .config,
                                    detail: "files remain"
                                ),
                                filesRemain: true
                            )
                        )
                        : .removed
                }
            )

            await #expect(throws: DevResetError.self) {
                try await devReset.run(profilePath: "/profile.toml")
            }
            #expect(
                calls.calls == [
                    "finish removals",
                    "list libraries",
                    "remove library-1",
                ]
            )
        }

        @Test(
            "an unreadable profile stops the reset before any library is touched"
        )
        func unreadableProfileStops() async {
            let calls = ResetCalls()
            let devReset = reset(
                calls,
                readProfile: { _ in throw ProfileUnreadable() }
            )

            await #expect(throws: ProfileUnreadable.self) {
                try await devReset.run(profilePath: "/profile.toml")
            }
            #expect(calls.calls.isEmpty)
        }
    }
#endif
