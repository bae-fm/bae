#if BAE_DEV_RESET
    import BaeKit

    extension AppDelegate {
        /// Run the `--dev-reset` launch, then open the library it created. A
        /// failure stops the reset where it happened and shows on the welcome
        /// screen; nothing after it runs.
        func runDevReset(profilePath: String, keyringReady: Bool) {
            guard keyringReady else {
                baeAppLogger.error(
                    "Dev reset needs the keyring, which didn't start"
                )
                screen = .welcome
                return
            }
            let services = requiredApplicationServices
            guard let home = baeAppProcessEnvironment["HOME"], !home.isEmpty
            else {
                preconditionFailure("HOME is unset, so ~/.bae has no location")
            }
            let reset = DevReset.live(
                host: services.host,
                home: home,
                deviceName: services.deviceName
            )
            screen = .loading
            Task {
                do {
                    let libraryId = try await reset.run(
                        profilePath: profilePath
                    )
                    libraries = try await services.host.listLibraries()
                    openLocalLibrary(id: libraryId)
                }
                catch {
                    baeAppLogger.error(
                        "Dev reset failed: \(String(reflecting: error))"
                    )
                    loadError = DisplayError(error)
                    screen = .welcome
                }
            }
        }
    }
#endif
