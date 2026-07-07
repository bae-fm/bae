#if BAE_OAUTH_PROVIDERS

import AuthenticationServices
import Foundation
import os.log

private let logger = Logger.bae("OAuthLinking")

enum OAuthLinkingError: Error, LocalizedError {
    case notConfigured
    case badAuthUrl
    case noCode
    case noState
    case unreadableConfig(String)
    case malformedConfig(String)
    case incompleteProviderConfig(String)
    case emptyConfig
    case noPresentationAnchor

    var errorDescription: String? {
        switch self {
        case .notConfigured:
            String(
                localized:
                    "Cloud sign-in isn't configured on this build (no oauth-creds.json)."
            )
        case .badAuthUrl:
            String(localized: "The provider returned an invalid authorization URL.")
        case .noCode:
            String(localized: "Authorization finished without a code.")
        case .noState:
            String(localized: "Authorization finished without a state.")
        case .unreadableConfig(let message):
            String(localized: "Couldn't read oauth-creds.json: \(message)")
        case .malformedConfig(let message):
            String(localized: "oauth-creds.json is malformed: \(message)")
        case .incompleteProviderConfig(let provider):
            String(
                localized:
                    "oauth-creds.json is missing client_id or redirect_uri for \(provider)."
            )
        case .emptyConfig:
            String(
                localized: "oauth-creds.json does not contain any provider credentials."
            )
        case .noPresentationAnchor:
            String(localized: "Couldn't present cloud sign-in from the current window.")
        }
    }
}

/// Per-device OAuth client config for providers that need it (Google Drive,
/// Dropbox, OneDrive). Loaded from a gitignored `oauth-creds.json` in the app
/// bundle — bae and coven ship no credentials; you register your own OAuth
/// app's client id and the redirect URI from its console (an installed-app
/// client, so the redirect is a custom scheme, not a localhost loopback). When
/// the file is absent, OAuth linking is unavailable and onboarding says so.
///
/// File shape:
/// ```json
/// {
///   "google_drive": {
///     "client_id": "<id>.apps.googleusercontent.com",
///     "redirect_uri": "com.googleusercontent.apps.<id>:/oauth2redirect"
///   }
/// }
/// ```
struct OAuthLinking {
    struct ProviderConfig {
        let clientId: String
        let clientSecret: String?
        let redirectUri: String
    }

    let providers: [String: ProviderConfig]

    /// Load the bundled creds. Returns nil when no `oauth-creds.json` is
    /// bundled, in which case OAuth linking stays unavailable.
    static func load() throws -> OAuthLinking? {
        guard let url = Bundle.main.url(forResource: "oauth-creds", withExtension: "json") else {
            logger.debug("oauth-creds.json not bundled; OAuth linking unavailable")
            return nil
        }
        let data: Data
        do {
            data = try Data(contentsOf: url)
        }
        catch {
            throw OAuthLinkingError.unreadableConfig(error.localizedDescription)
        }
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

        var providers: [String: ProviderConfig] = [:]
        for (provider, fields) in raw {
            guard let clientId = fields["client_id"],
                let redirectUri = fields["redirect_uri"]
            else {
                throw OAuthLinkingError.incompleteProviderConfig(provider)
            }
            providers[provider] = ProviderConfig(
                clientId: clientId,
                clientSecret: fields["client_secret"],
                redirectUri: redirectUri
            )
        }
        guard !providers.isEmpty else {
            throw OAuthLinkingError.emptyConfig
        }
        return OAuthLinking(providers: providers)
    }

    /// Register the client ids with the bridge so coven can build authorization
    /// URLs and refresh tokens during sync. Idempotent — the bridge stores them
    /// once. Call once at launch.
    func register() throws {
        var credsForBridge: [String: [String: String]] = [:]
        for (provider, config) in providers {
            var entry = ["client_id": config.clientId]
            if let clientSecret = config.clientSecret {
                entry["client_secret"] = clientSecret
            }
            credsForBridge[provider] = entry
        }
        let json = try JSONSerialization.data(withJSONObject: credsForBridge)
        guard let jsonString = String(bytes: json, encoding: .utf8) else {
            preconditionFailure("JSONSerialization produced non-UTF-8 data")
        }
        try setOauthClientCreds(credsJson: jsonString)
    }

    /// Run the OAuth flow for `provider` and return the token JSON to hand to
    /// `restoreFromCode`. Opens the system auth session; throws on cancel/fail.
    @MainActor
    func authorize(
        provider: BridgeCloudProvider,
        presentationAnchor: ASPresentationAnchor
    ) async throws -> String {
        guard let key = Self.providerKey(provider),
            let config = providers[key]
        else {
            throw OAuthLinkingError.notConfigured
        }
        let request = try oauthBegin(provider: provider, redirectUri: config.redirectUri)
        guard let authUrl = URL(string: request.authUrl) else {
            throw OAuthLinkingError.badAuthUrl
        }
        // The callback scheme is the redirect URI up to the first colon.
        let callbackScheme = String(config.redirectUri.prefix { $0 != ":" })
        let callback = try await WebAuthCoordinator(
            presentationAnchor: presentationAnchor
        )
        .authorize(
            url: authUrl,
            callbackScheme: callbackScheme
        )
        return try oauthComplete(
            provider: provider,
            code: callback.code,
            state: callback.state,
            requestId: request.requestId,
            redirectUri: config.redirectUri
        )
    }

    /// coven keys OAuth client creds by provider name.
    private static func providerKey(_ provider: BridgeCloudProvider) -> String? {
        switch provider {
        case .googleDrive: "google_drive"
        case .dropbox: "dropbox"
        case .oneDrive: "onedrive"
        case .s3, .cloudKit: nil
        }
    }
}

/// Drives one `ASWebAuthenticationSession` to completion. The session holds its
/// presentation-context provider weakly and is itself easy to let deallocate
/// mid-flow, so the coordinator pins itself (and the session) alive from
/// `start()` until the callback fires, then releases — no leak, exactly one
/// continuation resume.
@MainActor
final class WebAuthCoordinator: NSObject, ASWebAuthenticationPresentationContextProviding {
    struct Callback {
        let code: String
        let state: String
    }

    private struct ActiveSession {
        let session: ASWebAuthenticationSession
        /// Keeps the presentation provider alive until the session callback runs.
        let coordinator: WebAuthCoordinator
    }

    private let presentationAnchor: ASPresentationAnchor
    private var activeSession: ActiveSession?

    init(presentationAnchor: ASPresentationAnchor) {
        self.presentationAnchor = presentationAnchor
    }

    func authorize(url: URL, callbackScheme: String) async throws -> Callback {
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
                    defer {
                        self?.activeSession = nil
                    }
                    if let error {
                        continuation.resume(throwing: error)
                        return
                    }
                    guard let callbackURL,
                        let queryItems = URLComponents(
                            url: callbackURL,
                            resolvingAgainstBaseURL: false
                        )?
                        .queryItems
                    else {
                        continuation.resume(throwing: OAuthLinkingError.noCode)
                        return
                    }
                    guard
                        let code =
                            queryItems
                            .first(where: { $0.name == "code" })?
                            .value
                    else {
                        continuation.resume(throwing: OAuthLinkingError.noCode)
                        return
                    }
                    guard
                        let state =
                            queryItems
                            .first(where: { $0.name == "state" })?
                            .value
                    else {
                        continuation.resume(throwing: OAuthLinkingError.noState)
                        return
                    }
                    continuation.resume(returning: Callback(code: code, state: state))
                }
                session.presentationContextProvider = self
                session.prefersEphemeralWebBrowserSession = false
                self.activeSession = ActiveSession(session: session, coordinator: self)
                if !session.start() {
                    self.activeSession = nil
                    continuation.resume(throwing: OAuthLinkingError.badAuthUrl)
                }
            }
        } onCancel: {
            Task { @MainActor [weak self] in
                self?.activeSession?.session.cancel()
            }
        }
    }

    func presentationAnchor(for session: ASWebAuthenticationSession) -> ASPresentationAnchor {
        presentationAnchor
    }
}

#endif
