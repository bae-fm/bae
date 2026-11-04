#if BAE_DEV_RESET
    import BaeKit

    extension AppDelegate {
        /// Run the `--dev-reset` launch, then open the library it created.
        /// The loading screen shows each step core reports; a failure stops
        /// the reset where it happened and shows on the welcome screen;
        /// nothing after it runs.
        func runDevReset(profilePath: String) {
            let services = requiredApplicationServices
            let steps = LibrarySetupSteps(host: services.host)
            setupSteps = steps
            screen = .loading(.settingUp)
            Task {
                defer { setupSteps = nil }
                do {
                    let libraryId = try await services.host.devReset(
                        profile: readDevResetProfile(path: profilePath),
                        deviceName: services.deviceName,
                        setup: steps.setup
                    )
                    libraries = try await services.host.listLibraries()
                    openLocalLibrary(id: libraryId)
                }
                catch {
                    baeAppLogger.error(
                        "Dev reset failed: \(String(reflecting: error))"
                    )
                    loadError = LoadError(.devReset, error)
                    screen = .home
                }
            }
        }
    }
#endif
