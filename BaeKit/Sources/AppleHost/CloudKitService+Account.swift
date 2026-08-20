#if BAE_CLOUDKIT

    import BaeKit
    import CloudKit

    extension CloudKitService {
        /// Pre-flight check the sync setup wizard runs before persisting CloudKit
        /// as the provider. Throws if iCloud isn't available — most often "the
        /// user has iCloud signed out on this device", which the system-level
        /// `accountStatus()` API is exactly designed to report. Without this
        /// check, `useCloudkit()` happily writes `provider: CloudKit` to YAML and
        /// the user discovers via the reconnect banner after the first failed
        /// sync cycle.
        public func checkAccountAvailable() async throws {
            let status: CKAccountStatus
            do {
                status = try await accountStatus()
            }
            catch {
                // `displayLine` is optional because core reports a cancellation
                // as having no line. `accountStatus()` reports CloudKit's own
                // error, which always has one, so the fallback is the sentence
                // without a detail rather than `Optional("…")` inside it.
                throw CloudKitError.Storage(
                    msg: error.displayLine.map {
                        "Couldn't check iCloud account status: \($0)"
                    } ?? "Couldn't check iCloud account status."
                )
            }
            let unavailableReason: String
            switch status {
            case .available:
                return
            case .noAccount:
                unavailableReason =
                    "No iCloud account is signed in on this device. Open System Settings → Apple ID to sign in, then try again."
            case .restricted:
                unavailableReason =
                    "iCloud is restricted on this device (parental controls or MDM). bae can't use it for sync."
            case .couldNotDetermine:
                unavailableReason =
                    "Couldn't determine iCloud account status. Check your network and try again."
            case .temporarilyUnavailable:
                unavailableReason =
                    "iCloud is temporarily unavailable. Try again in a moment."
            @unknown default:
                unavailableReason =
                    "Unexpected iCloud account status (\(status.rawValue))."
            }
            throw CloudKitError.Storage(msg: unavailableReason)
        }
    }

#endif
