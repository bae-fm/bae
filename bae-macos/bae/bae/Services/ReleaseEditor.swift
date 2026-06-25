import Foundation

/// Release-level mutations and the remote-cover / edit-seed fetches
/// the editor needs to populate forms. Covers cover changes, pinning,
/// deletion, primary-release selection, re-identify, metadata
/// seeding/save/reset, and the remote-cover gallery fetch.
final class ReleaseEditor: Sendable, Observable {
    let changeCover:
        @Sendable (
            _ albumId: String, _ releaseId: String,
            _ selection: BridgeCoverSelection
        ) async throws -> Void
    let unpinRelease: @Sendable (_ releaseId: String) async throws -> Void
    let manageRelease:
        @Sendable (_ releaseId: String, _ pin: Bool) async throws -> Void
    let unmanageRelease:
        @Sendable (_ releaseId: String, _ newPath: String) async throws -> Void
    let deleteRelease: @Sendable (_ releaseId: String) -> Void
    let setPrimaryRelease:
        @Sendable (_ albumId: String, _ releaseId: String) throws -> Void
    let reIdentifyRelease:
        @Sendable (_ releaseId: String, _ identityChoice: BridgeIdentityChoice)
            async throws -> String
    let seedReleaseEdit:
        @Sendable (_ releaseId: String) async throws -> BridgeRawReleaseEdit
    let updateReleaseMetadataUserEdit:
        @Sendable (_ releaseId: String, _ edit: BridgeReleaseUserEdit)
            async throws -> Void
    let resetMetadataToSource:
        @Sendable (_ releaseId: String) async throws -> BridgeReleaseUserEdit
    let fetchRemoteCovers:
        @Sendable (_ releaseId: String) async throws -> [BridgeRemoteCover]

    init(
        changeCover:
            @escaping @Sendable (String, String, BridgeCoverSelection)
            async throws -> Void = { _, _, _ in },
        unpinRelease: @escaping @Sendable (String) async throws -> Void = {
            _ in
        },
        manageRelease:
            @escaping @Sendable (String, Bool) async throws -> Void =
            { _, _ in },
        unmanageRelease:
            @escaping @Sendable (String, String) async throws -> Void = {
                _,
                _ in
            },
        deleteRelease: @escaping @Sendable (String) -> Void = { _ in },
        setPrimaryRelease: @escaping @Sendable (String, String) throws -> Void =
            { _, _ in },
        reIdentifyRelease:
            @escaping @Sendable (String, BridgeIdentityChoice) async throws ->
            String = { _, _ in "" },
        seedReleaseEdit:
            @escaping @Sendable (String) async throws -> BridgeRawReleaseEdit =
            { _ in throw StubError.notImplemented },
        updateReleaseMetadataUserEdit:
            @escaping @Sendable (String, BridgeReleaseUserEdit) async throws ->
            Void = { _, _ in },
        resetMetadataToSource:
            @escaping @Sendable (String) async throws -> BridgeReleaseUserEdit =
            { _ in throw StubError.notImplemented },
        fetchRemoteCovers:
            @escaping @Sendable (String) async throws -> [BridgeRemoteCover] = {
                _ in []
            }
    ) {
        self.changeCover = changeCover
        self.unpinRelease = unpinRelease
        self.manageRelease = manageRelease
        self.unmanageRelease = unmanageRelease
        self.deleteRelease = deleteRelease
        self.setPrimaryRelease = setPrimaryRelease
        self.reIdentifyRelease = reIdentifyRelease
        self.seedReleaseEdit = seedReleaseEdit
        self.updateReleaseMetadataUserEdit = updateReleaseMetadataUserEdit
        self.resetMetadataToSource = resetMetadataToSource
        self.fetchRemoteCovers = fetchRemoteCovers
    }

    convenience init(handle: any AppHandleProtocol) {
        self.init(
            changeCover: {
                try await handle.changeCover(
                    albumId: $0,
                    releaseId: $1,
                    selection: $2
                )
            },
            unpinRelease: { try await handle.unpinRelease(releaseId: $0) },
            manageRelease: {
                try await handle.manageRelease(releaseId: $0, pin: $1)
            },
            unmanageRelease: {
                try await handle.unmanageRelease(releaseId: $0, newPath: $1)
            },
            deleteRelease: { handle.deleteRelease(releaseId: $0) },
            setPrimaryRelease: {
                try handle.setPrimaryRelease(albumId: $0, releaseId: $1)
            },
            reIdentifyRelease: {
                try await handle.reIdentifyRelease(
                    releaseId: $0,
                    identityChoice: $1
                )
            },
            seedReleaseEdit: {
                try await handle.seedReleaseEdit(releaseId: $0)
            },
            updateReleaseMetadataUserEdit: {
                try await handle.updateReleaseMetadataUserEdit(
                    releaseId: $0,
                    edit: $1
                )
            },
            resetMetadataToSource: {
                try await handle.resetMetadataToSource(releaseId: $0)
            },
            fetchRemoteCovers: {
                try await handle.fetchRemoteCovers(releaseId: $0)
            }
        )
    }

    // periphery:ignore
    static let stub = ReleaseEditor()
}
