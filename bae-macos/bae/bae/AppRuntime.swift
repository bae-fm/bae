import Foundation

/// What this process is running as: the installed app, an Xcode preview, or a
/// unit-test host, which starts none of the app's services.
enum AppRuntime: Equatable {
    case application
    case preview
    case testHost

    init(environment: [String: String]) {
        if Self.isPreview(environment: environment) {
            self = .preview
        }
        else if Self.isTestHost(environment: environment) {
            self = .testHost
        }
        else {
            self = .application
        }
    }

    var startsApplicationServices: Bool {
        self == .application
    }

    private static func isPreview(environment: [String: String]) -> Bool {
        environment["XCODE_RUNNING_FOR_PREVIEWS"] == "1"
    }

    private static func isTestHost(environment: [String: String]) -> Bool {
        #if DEBUG
            if environment["BAE_UI_TESTING"] == "1" {
                return false
            }
        #endif
        return environment["XCTestConfigurationFilePath"] != nil
    }

    #if BAE_TEST_KEYRING
        /// A UI test's launch, whose keys stay in an in-memory keychain.
        static func usesTestKeyring(environment: [String: String]) -> Bool {
            environment["BAE_UI_TESTING"] == "1"
        }
    #endif

    #if DEBUG
        static func createsLibraryForUITesting(
            environment: [String: String]
        ) -> Bool {
            environment["BAE_UI_TESTING_CREATE_LIBRARY"] == "1"
        }

        /// The JSON file naming the state a UI test opens the app on, which
        /// bae-core's `LibraryFixture` reads.
        static func libraryFixtureForUITesting(
            environment: [String: String]
        ) -> String? {
            environment["BAE_UI_TESTING_LIBRARY_FIXTURE"]
        }
    #endif
}
