#if BAE_CLOUDKIT
    import BaeKit
    import CloudKit
    import Foundation

    enum CloudKitFailure {
        /// Translate a CloudKit operation failure into the message core carries
        /// as the failure's diagnostic. The four classified `CKError` codes are
        /// recovery copy a user acts on — bae shows them under core's category
        /// line whenever a sync cycle fails — so they are localized. The
        /// fallback names the operation and CloudKit's own description, which is
        /// a diagnostic for a log or a bug report, and stays English.
        static func message(_ error: Error, op: String) -> String {
            guard let ckError = error as? CKError else {
                // Optional because core reports a cancellation as "no line to
                // show" — then the operation label is the whole message, and
                // interpolating the optional would render `Optional("…")`.
                return error.displayLine.map { "\(op) failed: \($0)" }
                    ?? "\(op) failed."
            }
            switch ckError.code {
            case .notAuthenticated:
                return String(
                    localized:
                        "You're not signed into iCloud. Open System Settings → Apple ID to sign in, then try again."
                )
            case .quotaExceeded:
                return String(
                    localized:
                        "Your iCloud storage is full. Free up space in System Settings → Apple ID → iCloud to keep syncing."
                )
            case .permissionFailure:
                return String(
                    localized:
                        "bae doesn't have permission to use iCloud. Open System Settings → Apple ID → iCloud → Apps Using iCloud and turn bae on."
                )
            case .zoneNotFound, .userDeletedZone:
                return String(
                    localized:
                        "The iCloud sync zone is gone. Reconnect in sync settings to recreate it."
                )
            default:
                // Concrete `CKError`, not a `LocalizedFailure`, so `DisplayError`
                // always resolves it to `localizedDescription` — CloudKit's own
                // localized message. Reading it directly keeps the sentence
                // non-optional instead of rendering `Optional("…")`; the
                // `displayLine` detour exists for `any Error`, where a bridge
                // failure would otherwise print its reflected enum.
                return "\(op) failed: \(ckError.localizedDescription)"
            }
        }

    }
#endif
