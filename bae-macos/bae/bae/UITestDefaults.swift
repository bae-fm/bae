#if DEBUG
    import Foundation
    import ObjectiveC

    /// The defaults a UI test names, opened once; `nil` outside a UI test.
    /// `UserDefaults` is thread-safe, which its declaration doesn't state.
    nonisolated(unsafe) private let uiTestDefaults: UserDefaults? = {
        guard
            let path = AppRuntime.defaultsForUITesting(
                environment: baeAppProcessEnvironment
            )
        else {
            return nil
        }
        guard let defaults = UserDefaults(suiteName: path) else {
            preconditionFailure("UI test defaults at \(path) don't open")
        }
        return defaults
    }()

    /// Runs the app under a UI test on defaults of the test's own: the
    /// property list it names, in place of the app's user defaults, which are
    /// the person's. Those are the defaults domain the bundle identifier
    /// names, which cfprefsd keeps in the user's own Library whatever the
    /// launch environment says (`HOME` and `CFFIXED_USER_HOME` move neither),
    /// so only the process can point its readers elsewhere. They all ask
    /// `UserDefaults.standard` (the app's preferences, AppKit's window frames,
    /// Sparkle, which keeps what it gets), so replacing that one getter before
    /// any of them asks moves them all, and the app's code reads defaults the
    /// same way under a test as outside one.
    enum UITestDefaults {
        /// Make `UserDefaults.standard` answer the test's defaults, when a UI
        /// test names them. Runs first in the process, before anything reads
        /// a default.
        static func install() {
            guard uiTestDefaults != nil else {
                return
            }
            guard
                let standard = class_getClassMethod(
                    UserDefaults.self,
                    #selector(getter: UserDefaults.standard)
                ),
                let replacement = class_getClassMethod(
                    UserDefaults.self,
                    #selector(getter: UserDefaults.uiTestStandard)
                )
            else {
                preconditionFailure("UserDefaults has no standard getter")
            }
            method_exchangeImplementations(standard, replacement)
        }
    }

    extension UserDefaults {
        /// What `standard` answers once `UITestDefaults.install` swaps it in.
        @objc
        fileprivate class var uiTestStandard: UserDefaults {
            guard let uiTestDefaults else {
                preconditionFailure("UI test defaults read without a UI test")
            }
            return uiTestDefaults
        }
    }
#endif
