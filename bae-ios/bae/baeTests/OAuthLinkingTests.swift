#if BAE_OAUTH_PROVIDERS

import BaeKit
import Foundation
import Testing

@testable import bae

/// Covers the pure halves of provider sign-in: parsing the bundled
/// `oauth-creds.json` into the applications and redirects the host registers,
/// and reading the redirect scheme the sign-in sheet waits for. The live sheet
/// (`ASWebAuthenticationSession`) and the bridge registration cross a system
/// service or the FFI boundary and aren't unit-tested.
@Suite("OAuthLinking.parse")
struct OAuthLinkingParseTests {
    private func data(_ json: String) -> Data {
        Data(json.utf8)
    }

    @Test("both providers' applications and redirects are read")
    func parsesBothProviders() throws {
        let linking = try OAuthLinking.parse(
            data(
                """
                {
                  "dropbox": {
                    "client_id": "dropbox-key",
                    "redirect_uri": "bae-dropbox:/oauth"
                  },
                  "onedrive": {
                    "client_id": "onedrive-id",
                    "redirect_uri": "bae-onedrive:/oauth"
                  }
                }
                """
            )
        )

        #expect(linking.clients.dropboxAppKey == "dropbox-key")
        #expect(linking.clients.onedriveClientId == "onedrive-id")
        #expect(linking.redirects.dropbox == "bae-dropbox:/oauth")
        #expect(linking.redirects.onedrive == "bae-onedrive:/oauth")
    }

    @Test("a missing provider is named as incomplete")
    func rejectsMissingProvider() {
        let error = #expect(throws: OAuthLinkingError.self) {
            try OAuthLinking.parse(
                data(
                    #"{ "dropbox": { "client_id": "k", "redirect_uri": "s:/cb" } }"#
                )
            )
        }
        guard case .incompleteProviderConfig(let provider)? = error else {
            Issue.record(
                "Expected .incompleteProviderConfig, got \(String(describing: error))"
            )
            return
        }
        #expect(provider == "onedrive")
    }

    @Test("a provider missing its redirect is named as incomplete")
    func rejectsMissingRedirect() {
        let error = #expect(throws: OAuthLinkingError.self) {
            try OAuthLinking.parse(
                data(
                    #"{ "dropbox": { "client_id": "k" }, "onedrive": { "client_id": "o", "redirect_uri": "s:/cb" } }"#
                )
            )
        }
        guard case .incompleteProviderConfig(let provider)? = error else {
            Issue.record(
                "Expected .incompleteProviderConfig, got \(String(describing: error))"
            )
            return
        }
        #expect(provider == "dropbox")
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

@Suite("WebAuthPresenter.callbackScheme")
struct WebAuthCallbackSchemeTests {
    @Test("the scheme comes from the request's redirect_uri")
    func readsRedirectScheme() throws {
        let url = try #require(
            URL(
                string:
                    "https://www.dropbox.com/oauth2/authorize?response_type=code&redirect_uri=bae-dropbox%3A%2Foauth&state=s"
            )
        )
        #expect(try WebAuthPresenter.callbackScheme(of: url) == "bae-dropbox")
    }

    @Test("a request without a redirect_uri is refused")
    func rejectsMissingRedirect() throws {
        let url = try #require(
            URL(string: "https://www.dropbox.com/oauth2/authorize?state=s")
        )
        #expect(throws: WebAuthFailure.self) {
            try WebAuthPresenter.callbackScheme(of: url)
        }
    }
}

#endif
