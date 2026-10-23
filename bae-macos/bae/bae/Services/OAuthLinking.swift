#if BAE_OAUTH_PROVIDERS
    import BaeKit
    import Foundation
    import os.log

    private let logger = Logger.bae("OAuthLinking")

    enum OAuthLinkingError: Error, LocalizedError {
        case unreadableConfig(String)
        case malformedConfig(String)
        case missingClientId(String)

        var errorDescription: String? {
            switch self {
            case .unreadableConfig(let message):
                String(localized: "Couldn't read oauth-creds.json: \(message)")
            case .malformedConfig(let message):
                String(localized: "oauth-creds.json is malformed: \(message)")
            case .missingClientId(let provider):
                String(
                    localized:
                        "oauth-creds.json is missing client_id for \(provider)."
                )
            }
        }
    }

    /// This build's OAuth applications for Dropbox and OneDrive, loaded from
    /// a gitignored `oauth-creds.json` in the app bundle — bae and coven ship
    /// no credentials. Register each as a desktop
    /// application whose redirect is coven's loopback callback,
    /// `http://localhost:19284/callback`: coven's desktop sign-in opens the
    /// browser and listens there. A provider with no entry isn't registered,
    /// and coven says so when it is asked for; without the file, provider
    /// sign-in is unavailable.
    ///
    /// File shape, every entry optional:
    /// ```json
    /// {
    ///   "dropbox": { "client_id": "<app-key>" },
    ///   "onedrive": { "client_id": "<client-id>" }
    /// }
    /// ```
    struct OAuthLinking {
        let clients: BridgeOAuthClients

        /// Load the bundled config, or nil when none is bundled.
        static func load() throws -> OAuthLinking? {
            guard
                let url = Bundle.main.url(
                    forResource: "oauth-creds",
                    withExtension: "json"
                )
            else {
                logger.debug(
                    "oauth-creds.json not bundled; provider sign-in unavailable"
                )
                return nil
            }
            let data: Data
            do {
                data = try Data(contentsOf: url)
            }
            catch {
                throw OAuthLinkingError.unreadableConfig(
                    error.localizedDescription
                )
            }
            return try parse(data)
        }

        /// Decode the raw `oauth-creds.json` bytes. Split out from `load()` so
        /// the shape rules are exercised without a file bundled into
        /// `Bundle.main`.
        static func parse(_ data: Data) throws -> OAuthLinking {
            let decoded: Any
            do {
                decoded = try JSONSerialization.jsonObject(with: data)
            }
            catch {
                throw OAuthLinkingError.malformedConfig(
                    error.localizedDescription
                )
            }
            guard let raw = decoded as? [String: [String: String]] else {
                throw OAuthLinkingError.malformedConfig(
                    "Expected a JSON object whose values are provider credential objects."
                )
            }
            func clientId(_ provider: String) throws -> String? {
                guard let fields = raw[provider] else { return nil }
                guard let clientId = fields["client_id"] else {
                    throw OAuthLinkingError.missingClientId(provider)
                }
                return clientId
            }
            return OAuthLinking(
                clients: BridgeOAuthClients(
                    dropboxAppKey: try clientId("dropbox"),
                    onedriveClientId: try clientId("onedrive")
                )
            )
        }

        /// Register the applications with the host, once at launch, before any
        /// restore, join or sign-in runs.
        func register(host: BridgeHost) {
            host.setOauthClients(clients: clients)
        }
    }
#endif
