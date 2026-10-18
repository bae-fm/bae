#if BAE_OAUTH_PROVIDERS

import AuthenticationServices
import BaeKit
import Foundation
import UIKit
import os.log

private let logger = Logger.bae("OAuthLinking")

enum OAuthLinkingError: Error, LocalizedError {
    case unreadableConfig(String)
    case malformedConfig(String)
    case incompleteProviderConfig(String)

    var errorDescription: String? {
        switch self {
        case .unreadableConfig(let message):
            String(localized: "Couldn't read oauth-creds.json: \(message)")
        case .malformedConfig(let message):
            String(localized: "oauth-creds.json is malformed: \(message)")
        case .incompleteProviderConfig(let provider):
            String(
                localized:
                    "oauth-creds.json is missing client_id or redirect_uri for \(provider)."
            )
        }
    }
}

/// This build's OAuth applications for Dropbox and OneDrive, loaded from a
/// gitignored `oauth-creds.json` in the app bundle — bae and coven ship no
/// credentials; you register your own apps (an installed-app client, so each
/// redirect is a custom scheme) and put their ids and redirects here. When the
/// file is absent, provider sign-in is unavailable and coven says so when it is
/// asked for.
///
/// File shape, both providers required:
/// ```json
/// {
///   "dropbox": { "client_id": "<app-key>", "redirect_uri": "bae-dropbox:/oauth" },
///   "onedrive": { "client_id": "<client-id>", "redirect_uri": "bae-onedrive:/oauth" }
/// }
/// ```
struct OAuthLinking {
    let clients: BridgeOAuthClients
    let redirects: BridgeOAuthRedirects

    /// Load the bundled config, or nil when none is bundled.
    static func load() throws -> OAuthLinking? {
        guard
            let url = Bundle.main.url(
                forResource: "oauth-creds",
                withExtension: "json"
            )
        else {
            logger.debug("oauth-creds.json not bundled; provider sign-in unavailable")
            return nil
        }
        let data: Data
        do {
            data = try Data(contentsOf: url)
        }
        catch {
            throw OAuthLinkingError.unreadableConfig(error.localizedDescription)
        }
        return try parse(data)
    }

    /// Decode the raw `oauth-creds.json` bytes. Split out from `load()` so the
    /// shape rules are exercised without a file bundled into `Bundle.main`.
    static func parse(_ data: Data) throws -> OAuthLinking {
        let decoded: Any
        do {
            decoded = try JSONSerialization.jsonObject(with: data)
        }
        catch {
            throw OAuthLinkingError.malformedConfig(error.localizedDescription)
        }
        guard let raw = decoded as? [String: [String: String]] else {
            throw OAuthLinkingError.malformedConfig(
                "Expected a JSON object whose values are provider credential objects."
            )
        }
        func app(_ provider: String) throws -> (id: String, redirect: String) {
            guard let fields = raw[provider],
                let clientId = fields["client_id"],
                let redirectUri = fields["redirect_uri"]
            else {
                throw OAuthLinkingError.incompleteProviderConfig(provider)
            }
            return (clientId, redirectUri)
        }
        let dropbox = try app("dropbox")
        let oneDrive = try app("onedrive")
        return OAuthLinking(
            // Google admin sign-in is a desktop admin source.
            clients: BridgeOAuthClients(
                dropboxAppKey: dropbox.id,
                onedriveClientId: oneDrive.id,
                googleAdminClientId: nil
            ),
            redirects: BridgeOAuthRedirects(
                dropbox: dropbox.redirect,
                onedrive: oneDrive.redirect
            )
        )
    }

    /// Register the applications and the system sign-in sheet with the host,
    /// once at launch, before any restore, join or sign-in runs.
    func register(host: BridgeHost) {
        host.setOauthClients(clients: clients)
        host.setOauthPresenter(
            redirects: redirects,
            callback: WebAuthPresenter()
        )
    }
}

/// Why the system sign-in sheet couldn't produce a redirect. Diagnostics only:
/// coven reports the failure with its own line.
enum WebAuthFailure: Error {
    case unreadableAuthorizationURL
    case noRedirectScheme
    case noKeyWindow
    case sessionDidNotStart
    case noRedirect
}

/// Presents coven's provider sign-in in `ASWebAuthenticationSession` and hands
/// back the complete redirect. coven builds the authorization URL and checks
/// the redirect; this only shows the page over the app's key window. When
/// coven stops waiting, the Swift task is cancelled and the sheet dismissed.
final class WebAuthPresenter: OAuthPresenterCallback, Sendable {
    func present(authorizationUrl: String) async throws -> String {
        do {
            guard let url = URL(string: authorizationUrl) else {
                throw WebAuthFailure.unreadableAuthorizationURL
            }
            let scheme = try Self.callbackScheme(of: url)
            let anchor = try await MainActor.run { try Self.keyWindow() }
            return try await WebAuthSession(anchor: anchor)
                .run(url: url, callbackScheme: scheme)
                .absoluteString
        }
        catch is CancellationError {
            throw BridgeOAuthPresentationError.Cancelled
        }
        catch ASWebAuthenticationSessionError.canceledLogin {
            throw BridgeOAuthPresentationError.Cancelled
        }
        catch {
            logger.error("Provider sign-in failed: \(error.localizedDescription)")
            throw BridgeOAuthPresentationError.Failed(
                detail: String(describing: error)
            )
        }
    }

    /// The custom scheme the provider redirects to, read off the
    /// `redirect_uri` the authorization request carries (RFC 6749 §4.1.1), so
    /// the session waits for the one redirect this sign-in registered.
    static func callbackScheme(of authorizationURL: URL) throws -> String {
        guard
            let redirect = URLComponents(
                url: authorizationURL,
                resolvingAgainstBaseURL: false
            )?
            .queryItems?
            .first(where: { $0.name == "redirect_uri" })?
            .value,
            let scheme = URL(string: redirect)?.scheme
        else {
            throw WebAuthFailure.noRedirectScheme
        }
        return scheme
    }

    @MainActor
    private static func keyWindow() throws -> ASPresentationAnchor {
        let window = UIApplication.shared.connectedScenes
            .compactMap { $0 as? UIWindowScene }
            .flatMap(\.windows)
            .first(where: \.isKeyWindow)
        guard let window else {
            throw WebAuthFailure.noKeyWindow
        }
        return window
    }
}

/// Drives one `ASWebAuthenticationSession` to completion — exactly one
/// continuation resume. The session holds its presentation-context provider
/// (this object) weakly; `run` keeps this object alive until it returns, which
/// is after the callback fires, and this object keeps the session alive.
@MainActor
final class WebAuthSession: NSObject,
    ASWebAuthenticationPresentationContextProviding
{
    private let anchor: ASPresentationAnchor
    private var active: ASWebAuthenticationSession?

    init(anchor: ASPresentationAnchor) {
        self.anchor = anchor
    }

    func run(url: URL, callbackScheme: String) async throws -> URL {
        try await withTaskCancellationHandler {
            try await withCheckedThrowingContinuation { continuation in
                guard !Task.isCancelled else {
                    continuation.resume(throwing: CancellationError())
                    return
                }
                let session = ASWebAuthenticationSession(
                    url: url,
                    callbackURLScheme: callbackScheme
                ) { [weak self] callbackURL, error in
                    defer { self?.active = nil }
                    if let error {
                        continuation.resume(throwing: error)
                        return
                    }
                    guard let callbackURL else {
                        continuation.resume(throwing: WebAuthFailure.noRedirect)
                        return
                    }
                    continuation.resume(returning: callbackURL)
                }
                session.presentationContextProvider = self
                session.prefersEphemeralWebBrowserSession = false
                active = session
                if !session.start() {
                    active = nil
                    continuation.resume(
                        throwing: WebAuthFailure.sessionDidNotStart
                    )
                }
            }
        } onCancel: {
            Task { @MainActor [weak self] in
                self?.active?.cancel()
            }
        }
    }

    nonisolated func presentationAnchor(
        for session: ASWebAuthenticationSession
    ) -> ASPresentationAnchor {
        MainActor.assumeIsolated { anchor }
    }
}

#endif
