#if DEBUG
    import BaeKit
    import SwiftUI

    // Fixtures for the Components leaf previews. Extends the shared `PreviewData`
    // namespace so the component previews draw their sample values from one place.
    @MainActor
    extension PreviewData {
        /// A UI-originated failure — prose the UI already localized, with no opaque
        /// detail to disclose. Renders the plain line with no disclosure row.
        static let displayErrorSimple = DisplayError(
            line:
                "Couldn't reach the sync service. Check your connection and try again."
        )

        /// A core diagnostic crossing the bridge: its category renders the generic
        /// line and the opaque Rust error chain rides along as copyable `detail`, so
        /// the disclosure row appears.
        static let displayErrorWithDetail: DisplayError = {
            guard
                let error = DisplayError(
                    BridgeError.Diagnostic(
                        category: .database,
                        detail: "no such table: releases (code 1)"
                    ) as any Error
                )
            else {
                fatalError("a Diagnostic error always yields a DisplayError")
            }
            return error
        }()
    }
#endif
