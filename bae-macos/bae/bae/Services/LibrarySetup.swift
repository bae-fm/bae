import BaeKit
import Foundation

/// A started join the view can await and abort: `join` blocks its worker until
/// the library is ready; `cancel` aborts the bridge-side work so a superseded
/// or abandoned join stops instead of completing against a stale library.
struct JoinOperation: Sendable {
    let join: @Sendable () throws -> BridgeLibrary
    let cancel: @Sendable () -> Void
}

// The bridge's free functions, bound at file scope where their names are
// unambiguous — inside `LibrarySetup` the field names shadow them.
private let bridgeDiscoverLibraries = discoverLibraries
private let bridgeCreateLibrary = createLibrary(name:)
private let bridgeDecodeRestoreCode = decodeRestoreCode(code:)
private let bridgeDecodeScannedInvite = decodeScannedInvite(scanned:)
private let bridgeRestoreFromCode = restoreFromCode(code:oauthTokenJson:)
private let bridgeGenerateJoinRequest = generateJoinRequest(email:)
private let bridgeJoinFromScannedInviteOperation =
    joinFromScannedInviteOperation(scanned:joinRequestCode:oauthTokenJson:)

/// The invite bundle behind what the joiner pasted.
///
/// The invite is a byte payload — the owner's signed join offer, not a
/// human-typed code — so it travels as base64 wherever it has to be text.
private func inviteBytes(pasted: String) throws -> Data {
    guard
        let bytes = Data(
            base64Encoded: pasted.trimmingCharacters(
                in: .whitespacesAndNewlines
            )
        )
    else {
        throw BridgeError.Diagnostic(
            category: .config,
            detail:
                "the pasted invite is not base64 an invite bundle could be read from"
        )
    }
    return bytes
}

// The OAuth functions exist only in builds whose bridge compiles the OAuth
// providers (the S3-only build omits them entirely); elsewhere the bindings
// fall back to the same not-implemented stubs the initializer defaults to,
// and the BAE_OAUTH_PROVIDERS-gated UI never calls them.
#if BAE_OAUTH_PROVIDERS
    private let bridgeOauthAuthorize = oauthAuthorize(provider:)
    private let bridgeOauthCancel = oauthCancel
    private let bridgeFetchAccountEmail = fetchAccountEmail(
        provider:
        oauthTokenJson:
    )
#else
    private let bridgeOauthAuthorize:
        @Sendable (BridgeCloudProvider) throws -> String = { _ in
            throw StubError.notImplemented
        }
    private let bridgeOauthCancel: @Sendable () -> Void = {}
    private let bridgeFetchAccountEmail:
        @Sendable (BridgeCloudProvider, String) throws -> String = { _, _ in
            throw StubError.notImplemented
        }
#endif

/// Pre-library operations the welcome flow drives: on-device discovery,
/// create, restore, join, the keychain restore codes, and provider OAuth.
/// Wraps the bridge's free functions and `KeychainService` behind one
/// injectable seam so previews never read or write real application data.
/// The code decoders ride along because previews hand out fixture codes only
/// this seam can "decode"; the pure validators (`validateRestoreConfig`,
/// `availableCloudProviders`) stay free functions — they touch no state.
final class LibrarySetup: Sendable, Observable {
    let discoverLibraries: @Sendable () throws -> [BridgeLibrary]
    /// Create a library named by core's default generator.
    let createLibrary: @Sendable () throws -> BridgeLibrary
    let decodeRestoreCode:
        @Sendable (_ code: String) throws -> BridgeRestoreCodeInfo
    /// Preview a pasted invite. The payload is bytes carried as base64, so both
    /// this and the join decode the same text the same way.
    let decodeInviteCode:
        @Sendable (_ pasted: String) throws -> BridgeInviteCodeInfo
    let restoreFromCode:
        @Sendable (_ code: String, _ oauthTokenJson: String?) throws ->
            BridgeLibrary
    let generateJoinRequest:
        @Sendable (_ email: String?) throws -> BridgeJoinRequest
    /// Start a join for a pasted invite; the returned operation runs it.
    let joinFromCode:
        @Sendable (
            _ pasted: String, _ joinRequestCode: String,
            _ oauthTokenJson: String?
        ) throws -> JoinOperation
    /// Restore codes any of the user's devices stored in the iCloud keychain.
    let fetchRestoreCodes: @Sendable () -> [(libraryId: String, code: String)]
    let deleteRestoreCode: @Sendable (_ libraryId: String) -> Void
    let oauthAuthorize:
        @Sendable (_ provider: BridgeCloudProvider) throws -> String
    let oauthCancel: @Sendable () -> Void
    let fetchAccountEmail:
        @Sendable (_ provider: BridgeCloudProvider, _ oauthTokenJson: String)
            throws -> String
    /// Reveal a library's folder in Finder — the broken-library row's "Show in
    /// Finder" action. Injected so previews get the no-op default instead of
    /// opening a real Finder window when the row's button is clicked.
    let revealInFinder: @Sendable (_ path: String) -> Void

    init(
        discoverLibraries: @escaping @Sendable () throws -> [BridgeLibrary] =
            { [] },
        createLibrary: @escaping @Sendable () throws -> BridgeLibrary = {
            throw StubError.notImplemented
        },
        decodeRestoreCode:
            @escaping @Sendable (String) throws -> BridgeRestoreCodeInfo = {
                _ in throw StubError.notImplemented
            },
        decodeInviteCode:
            @escaping @Sendable (String) throws -> BridgeInviteCodeInfo = {
                _ in throw StubError.notImplemented
            },
        restoreFromCode:
            @escaping @Sendable (String, String?) throws -> BridgeLibrary = {
                _,
                _ in throw StubError.notImplemented
            },
        generateJoinRequest:
            @escaping @Sendable (String?) throws -> BridgeJoinRequest = { _ in
                throw StubError.notImplemented
            },
        joinFromCode:
            @escaping @Sendable (String, String, String?) throws ->
            JoinOperation = { _, _, _ in throw StubError.notImplemented },
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
        fetchAccountEmail:
            @escaping @Sendable (BridgeCloudProvider, String) throws -> String =
            { _, _ in throw StubError.notImplemented },
        revealInFinder: @escaping @Sendable (String) -> Void = { _ in }
    ) {
        self.discoverLibraries = discoverLibraries
        self.createLibrary = createLibrary
        self.decodeRestoreCode = decodeRestoreCode
        self.decodeInviteCode = decodeInviteCode
        self.restoreFromCode = restoreFromCode
        self.generateJoinRequest = generateJoinRequest
        self.joinFromCode = joinFromCode
        self.fetchRestoreCodes = fetchRestoreCodes
        self.deleteRestoreCode = deleteRestoreCode
        self.oauthAuthorize = oauthAuthorize
        self.oauthCancel = oauthCancel
        self.fetchAccountEmail = fetchAccountEmail
        self.revealInFinder = revealInFinder
    }

    /// The production wiring: the bridge's free functions plus the keychain.
    /// The OAuth closures are wired in every build — the bridge always exports
    /// them — while the `BAE_OAUTH_PROVIDERS` flag gates the UI that calls.
    static let live = LibrarySetup(
        discoverLibraries: bridgeDiscoverLibraries,
        createLibrary: { try bridgeCreateLibrary(nil) },
        decodeRestoreCode: bridgeDecodeRestoreCode,
        decodeInviteCode: {
            try bridgeDecodeScannedInvite(inviteBytes(pasted: $0))
        },
        restoreFromCode: bridgeRestoreFromCode,
        generateJoinRequest: bridgeGenerateJoinRequest,
        joinFromCode: { pasted, joinRequestCode, oauthTokenJson in
            let operation = try bridgeJoinFromScannedInviteOperation(
                inviteBytes(pasted: pasted),
                joinRequestCode,
                oauthTokenJson
            )
            return JoinOperation(
                join: { try operation.join() },
                cancel: { operation.cancel() }
            )
        },
        fetchRestoreCodes: KeychainService.fetchAllRestoreCodes,
        deleteRestoreCode: KeychainService.deleteRestoreCode(libraryId:),
        oauthAuthorize: bridgeOauthAuthorize,
        oauthCancel: bridgeOauthCancel,
        fetchAccountEmail: bridgeFetchAccountEmail,
        revealInFinder: { SystemActions.revealInFinder(path: $0) }
    )

    #if DEBUG
        // periphery:ignore
        static let stub = LibrarySetup()
    #endif
}
