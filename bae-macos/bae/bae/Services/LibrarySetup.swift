import BaeKit
import Foundation

/// A started join the view can await and abort: `join` blocks its worker until
/// the library is ready; `cancel` aborts the bridge-side work so a superseded
/// or abandoned join stops instead of completing against a stale library.
struct JoinOperation: Sendable {
    let fingerprint: String
    let join:
        @Sendable (JoiningDeviceJoinProgressCallback) throws -> BridgeLibrary
    let cancel: @Sendable () throws -> Void
}

// The bridge's free functions, bound at file scope where their names are
// unambiguous — inside `LibrarySetup` the field names shadow them.
private let bridgeDiscoverLibraries = discoverLibraries
private let bridgeRemoveLocalLibrary = removeLocalLibrary(libraryId:)
private let bridgeCreateLibrary = createLibrary(name:)
private let bridgeDecodeRestoreCode = decodeRestoreCode(code:)
private let bridgeDecodeDevicePairingOffer = decodeDevicePairingOffer(code:)
private let bridgePendingDevicePairingJoin = pendingDevicePairingJoin
private let bridgeAbandonPendingDevicePairingJoin =
    abandonPendingDevicePairingJoin
private let bridgeRestoreFromCode = restoreFromCode(code:oauthTokenJson:)
private let bridgeJoinDevicePairingOperation =
    joinDevicePairingOperation(pairingCode:oauthTokenJson:)

// The OAuth functions exist only in builds whose bridge compiles the OAuth
// providers (the S3-only build omits them entirely); elsewhere the bindings
// fall back to the same not-implemented stubs the initializer defaults to,
// and the BAE_OAUTH_PROVIDERS-gated UI never calls them.
#if BAE_OAUTH_PROVIDERS
    private let bridgeOauthAuthorize = oauthAuthorize(provider:)
    private let bridgeOauthCancel = oauthCancel
#else
    private let bridgeOauthAuthorize:
        @Sendable (BridgeCloudProvider) throws -> String = { _ in
            throw StubError.notImplemented
        }
    private let bridgeOauthCancel: @Sendable () -> Void = {}
#endif

/// Pre-library operations the welcome flow drives: on-device discovery,
/// create, restore, join, the keychain restore codes, and provider OAuth.
/// Wraps the bridge's free functions and `KeychainService` behind one
/// injectable seam so previews never read or write real application data.
/// The code decoders ride along because previews hand out fixture codes only
/// this seam can "decode"; `availableCloudProviders` stays a free function — it
/// touches no state.
final class LibrarySetup: Sendable, Observable {
    let discoverLibraries: @Sendable () throws -> [BridgeLibrary]
    let removeLocalLibrary: @Sendable (_ libraryId: String) throws -> Void
    /// Create a library named by core's default generator.
    let createLibrary: @Sendable () throws -> BridgeLibrary
    let decodeRestoreCode:
        @Sendable (_ code: String) throws -> BridgeRestoreCodeInfo
    let decodeDevicePairingOffer:
        @Sendable (_ code: String) throws -> BridgeDevicePairingOffer
    let pendingDevicePairingJoin:
        @Sendable () throws -> BridgePendingDevicePairingJoin?
    let abandonPendingDevicePairingJoin: @Sendable () throws -> Void
    let restoreFromCode:
        @Sendable (_ code: String, _ oauthTokenJson: String?) throws ->
            BridgeLibrary
    let joinDevicePairing:
        @Sendable (_ code: String, _ oauthTokenJson: String?) async throws ->
            JoinOperation
    /// Restore codes any of the user's devices stored in the iCloud keychain.
    let fetchRestoreCodes: @Sendable () -> [(libraryId: String, code: String)]
    let deleteRestoreCode: @Sendable (_ libraryId: String) -> Void
    let oauthAuthorize:
        @Sendable (_ provider: BridgeCloudProvider) throws -> String
    let oauthCancel: @Sendable () -> Void
    /// Reveal a library's folder in Finder — the broken-library row's "Show in
    /// Finder" action. Injected so previews get the no-op default instead of
    /// opening a real Finder window when the row's button is clicked.
    let revealInFinder: @Sendable (_ path: String) -> Void

    init(
        discoverLibraries: @escaping @Sendable () throws -> [BridgeLibrary] =
            { [] },
        removeLocalLibrary: @escaping @Sendable (String) throws -> Void = {
            _ in throw StubError.notImplemented
        },
        createLibrary: @escaping @Sendable () throws -> BridgeLibrary = {
            throw StubError.notImplemented
        },
        decodeRestoreCode:
            @escaping @Sendable (String) throws -> BridgeRestoreCodeInfo = {
                _ in throw StubError.notImplemented
            },
        decodeDevicePairingOffer:
            @escaping @Sendable (String) throws -> BridgeDevicePairingOffer = {
                _ in throw StubError.notImplemented
            },
        pendingDevicePairingJoin:
            @escaping @Sendable () throws -> BridgePendingDevicePairingJoin? = {
                nil
            },
        abandonPendingDevicePairingJoin:
            @escaping @Sendable () throws -> Void = {},
        restoreFromCode:
            @escaping @Sendable (String, String?) throws -> BridgeLibrary = {
                _,
                _ in throw StubError.notImplemented
            },
        joinDevicePairing:
            @escaping @Sendable (String, String?) async throws -> JoinOperation =
            {
                _,
                _ in throw StubError.notImplemented
            },
        fetchRestoreCodes:
            @escaping @Sendable () -> [(libraryId: String, code: String)] = {
                []
            },
        deleteRestoreCode: @escaping @Sendable (String) -> Void = { _ in },
        oauthAuthorize:
            @escaping @Sendable (BridgeCloudProvider) throws -> String = {
                _ in throw StubError.notImplemented
            },
        oauthCancel: @escaping @Sendable () -> Void = {},
        revealInFinder: @escaping @Sendable (String) -> Void = { _ in }
    ) {
        self.discoverLibraries = discoverLibraries
        self.removeLocalLibrary = removeLocalLibrary
        self.createLibrary = createLibrary
        self.decodeRestoreCode = decodeRestoreCode
        self.decodeDevicePairingOffer = decodeDevicePairingOffer
        self.pendingDevicePairingJoin = pendingDevicePairingJoin
        self.abandonPendingDevicePairingJoin =
            abandonPendingDevicePairingJoin
        self.restoreFromCode = restoreFromCode
        self.joinDevicePairing = joinDevicePairing
        self.fetchRestoreCodes = fetchRestoreCodes
        self.deleteRestoreCode = deleteRestoreCode
        self.oauthAuthorize = oauthAuthorize
        self.oauthCancel = oauthCancel
        self.revealInFinder = revealInFinder
    }

    /// The production wiring: the bridge's free functions plus the keychain.
    /// The OAuth closures are wired in every build — the bridge always exports
    /// them — while the `BAE_OAUTH_PROVIDERS` flag gates the UI that calls.
    static func live() -> LibrarySetup {
        LibrarySetup(
            discoverLibraries: bridgeDiscoverLibraries,
            removeLocalLibrary: bridgeRemoveLocalLibrary,
            createLibrary: { try bridgeCreateLibrary(nil) },
            decodeRestoreCode: bridgeDecodeRestoreCode,
            decodeDevicePairingOffer: bridgeDecodeDevicePairingOffer,
            pendingDevicePairingJoin: bridgePendingDevicePairingJoin,
            abandonPendingDevicePairingJoin:
                bridgeAbandonPendingDevicePairingJoin,
            restoreFromCode: bridgeRestoreFromCode,
            joinDevicePairing: { code, oauthTokenJson in
                let operation = try await bridgeJoinDevicePairingOperation(
                    code,
                    oauthTokenJson
                )
                return JoinOperation(
                    fingerprint: operation.fingerprint(),
                    join: { progress in
                        try operation.join(progress: progress)
                    },
                    cancel: { try operation.cancel() }
                )
            },
            fetchRestoreCodes: KeychainService.fetchAllRestoreCodes,
            deleteRestoreCode: KeychainService.deleteRestoreCode(libraryId:),
            oauthAuthorize: bridgeOauthAuthorize,
            oauthCancel: bridgeOauthCancel,
            revealInFinder: { SystemActions.revealInFinder(path: $0) }
        )
    }

    #if DEBUG
        // periphery:ignore
        static func stub() -> LibrarySetup {
            LibrarySetup()
        }
    #endif
}
