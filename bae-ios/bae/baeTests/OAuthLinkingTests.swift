#if BAE_OAUTH_PROVIDERS

import Foundation
import Testing

@testable import bae

/// Covers the pure halves of the OAuth linking flow: parsing the bundled
/// `oauth-creds.json` and validating the provider's redirect callback. The
/// live flow (`load` reading `Bundle.main`, `authorize` driving an
/// `ASWebAuthenticationSession`, `register` calling the bridge) isn't
/// unit-tested — those cross a system service or the FFI boundary.
@Suite("OAuthLinking.parse")
struct OAuthLinkingParseTests {
    private func data(_ json: String) -> Data {
        Data(json.utf8)
    }

    @Test("a well-formed provider config is parsed")
    func parsesWellFormedConfig() throws {
        let linking = try OAuthLinking.parse(
            data(
                """
                {
                  "google_drive": {
                    "client_id": "client-123",
                    "redirect_uri": "com.example.app:/oauth2redirect"
                  }
                }
                """
            )
        )

        #expect(linking.providers.count == 1)
        let config = try #require(linking.providers["google_drive"])
        #expect(config.clientId == "client-123")
        #expect(config.redirectUri == "com.example.app:/oauth2redirect")
        #expect(config.clientSecret == nil)
    }

    @Test("an optional client secret is carried through")
    func parsesClientSecret() throws {
        let linking = try OAuthLinking.parse(
            data(
                """
                {
                  "dropbox": {
                    "client_id": "client-123",
                    "client_secret": "secret-456",
                    "redirect_uri": "com.example.app:/oauth2redirect"
                  }
                }
                """
            )
        )

        let config = try #require(linking.providers["dropbox"])
        #expect(config.clientSecret == "secret-456")
    }

    @Test("a provider missing client_id is rejected as incomplete")
    func rejectsMissingClientId() {
        let error = #expect(throws: OAuthLinkingError.self) {
            try OAuthLinking.parse(
                data(#"{ "dropbox": { "redirect_uri": "com.example.app:/cb" } }"#)
            )
        }
        guard case .incompleteProviderConfig(let provider)? = error else {
            Issue.record("Expected .incompleteProviderConfig, got \(String(describing: error))")
            return
        }
        #expect(provider == "dropbox")
    }

    @Test("a provider missing redirect_uri is rejected as incomplete")
    func rejectsMissingRedirectUri() {
        let error = #expect(throws: OAuthLinkingError.self) {
            try OAuthLinking.parse(
                data(#"{ "dropbox": { "client_id": "client-123" } }"#)
            )
        }
        guard case .incompleteProviderConfig? = error else {
            Issue.record("Expected .incompleteProviderConfig, got \(String(describing: error))")
            return
        }
    }

    @Test("an empty object has no providers and is rejected")
    func rejectsEmptyConfig() {
        let error = #expect(throws: OAuthLinkingError.self) {
            try OAuthLinking.parse(data("{}"))
        }
        guard case .emptyConfig? = error else {
            Issue.record("Expected .emptyConfig, got \(String(describing: error))")
            return
        }
    }

    @Test("a non-object shape is rejected as malformed")
    func rejectsWrongShape() {
        let error = #expect(throws: OAuthLinkingError.self) {
            try OAuthLinking.parse(data("[1, 2, 3]"))
        }
        guard case .malformedConfig? = error else {
            Issue.record("Expected .malformedConfig, got \(String(describing: error))")
            return
        }
    }

    @Test("invalid JSON is rejected as malformed")
    func rejectsInvalidJson() {
        let error = #expect(throws: OAuthLinkingError.self) {
            try OAuthLinking.parse(data("not json"))
        }
        guard case .malformedConfig? = error else {
            Issue.record("Expected .malformedConfig, got \(String(describing: error))")
            return
        }
    }
}

@Suite("WebAuthCoordinator.parseCallback")
struct WebAuthCallbackTests {
    @Test("a redirect with code and state yields both")
    func parsesCodeAndState() throws {
        let url = try #require(
            URL(string: "com.example.app:/oauth2redirect?code=the-code&state=the-state")
        )
        let callback = try WebAuthCoordinator.parseCallback(url)
        #expect(callback.code == "the-code")
        #expect(callback.state == "the-state")
    }

    @Test("a redirect without a code is rejected")
    func rejectsMissingCode() {
        let url = URL(string: "com.example.app:/oauth2redirect?state=the-state")
        let error = #expect(throws: OAuthLinkingError.self) {
            try WebAuthCoordinator.parseCallback(url)
        }
        guard case .noCode? = error else {
            Issue.record("Expected .noCode, got \(String(describing: error))")
            return
        }
    }

    @Test("a redirect with a code but no state is rejected")
    func rejectsMissingState() {
        let url = URL(string: "com.example.app:/oauth2redirect?code=the-code")
        let error = #expect(throws: OAuthLinkingError.self) {
            try WebAuthCoordinator.parseCallback(url)
        }
        guard case .noState? = error else {
            Issue.record("Expected .noState, got \(String(describing: error))")
            return
        }
    }

    @Test("a nil callback URL is rejected")
    func rejectsNilUrl() {
        let error = #expect(throws: OAuthLinkingError.self) {
            try WebAuthCoordinator.parseCallback(nil)
        }
        guard case .noCode? = error else {
            Issue.record("Expected .noCode, got \(String(describing: error))")
            return
        }
    }
}

#endif
