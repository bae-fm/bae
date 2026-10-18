#if BAE_OAUTH_PROVIDERS

    import BaeKit
    import Foundation
    import Testing

    @testable import bae

    /// Covers parsing the bundled `oauth-creds.json` into the applications the
    /// host registers. The registration itself crosses the FFI boundary and
    /// isn't unit-tested.
    @Suite("OAuthLinking.parse")
    struct OAuthLinkingParseTests {
        private func data(_ json: String) -> Data {
            Data(json.utf8)
        }

        @Test("every provider's application is read")
        func parsesEveryProvider() throws {
            let linking = try OAuthLinking.parse(
                data(
                    """
                    {
                      "dropbox": { "client_id": "dropbox-key" },
                      "onedrive": { "client_id": "onedrive-id" },
                      "google_admin": { "client_id": "google-id" }
                    }
                    """
                )
            )

            #expect(linking.clients.dropboxAppKey == "dropbox-key")
            #expect(linking.clients.onedriveClientId == "onedrive-id")
            #expect(linking.clients.googleAdminClientId == "google-id")
        }

        @Test("a provider with no entry isn't registered")
        func leavesAbsentProvidersUnregistered() throws {
            let linking = try OAuthLinking.parse(
                data(#"{ "dropbox": { "client_id": "dropbox-key" } }"#)
            )

            #expect(linking.clients.dropboxAppKey == "dropbox-key")
            #expect(linking.clients.onedriveClientId == nil)
            #expect(linking.clients.googleAdminClientId == nil)
        }

        @Test("an entry without its client id is named")
        func rejectsMissingClientId() {
            let error = #expect(throws: OAuthLinkingError.self) {
                try OAuthLinking.parse(
                    data(
                        #"{ "dropbox": { "client_id": "k" }, "onedrive": {} }"#
                    )
                )
            }
            guard case .missingClientId(let provider)? = error else {
                Issue.record(
                    "Expected .missingClientId, got \(String(describing: error))"
                )
                return
            }
            #expect(provider == "onedrive")
        }

        @Test("a non-object shape is rejected as malformed")
        func rejectsWrongShape() {
            let error = #expect(throws: OAuthLinkingError.self) {
                try OAuthLinking.parse(data("[1, 2, 3]"))
            }
            guard case .malformedConfig? = error else {
                Issue.record(
                    "Expected .malformedConfig, got \(String(describing: error))"
                )
                return
            }
        }

        @Test("invalid JSON is rejected as malformed")
        func rejectsInvalidJson() {
            let error = #expect(throws: OAuthLinkingError.self) {
                try OAuthLinking.parse(data("not json"))
            }
            guard case .malformedConfig? = error else {
                Issue.record(
                    "Expected .malformedConfig, got \(String(describing: error))"
                )
                return
            }
        }
    }

#endif
