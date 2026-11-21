import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacNotice` in each tone and layout.
    static let macNoticeScenes: [ShotScene] = [
        notice("mac.component.notice.neutral", height: 70) {
            MacNotice(
                title: "This library has no albums yet.",
                advice: nil,
                error: nil,
                tone: .neutral
            )
        },
        notice("mac.component.notice.info", height: 70) {
            MacNotice(
                title: "A newer version of this release is on Discogs.",
                advice: nil,
                error: nil,
                tone: .info
            )
        },
        notice("mac.component.notice.success", height: 70) {
            MacNotice(
                title: "Cloud connected.",
                advice: nil,
                error: nil,
                tone: .success
            )
        },
        notice("mac.component.notice.warning", height: 70) {
            MacNotice(
                title: "Unsent changes will be lost",
                advice: nil,
                error: nil,
                tone: .warning
            )
        },
        notice("mac.component.notice.warning-title-and-subtitle", height: 90) {
            MacNotice(
                title: "Mara Quill’s phone is out of date",
                advice: "Update bae on it to keep syncing.",
                error: nil,
                tone: .warning
            )
        },
        notice("mac.component.notice.danger", height: 70) {
            MacNotice(
                title: "Could not reach the cloud.",
                advice: nil,
                error: nil
            )
        },
        notice("mac.component.notice.danger-retry", height: 70) {
            MacNotice(
                title: "Could not reach the cloud.",
                advice: nil,
                error: nil,
                action: ErrorAction(perform: {})
            )
        },
        notice("mac.component.notice.danger-details-and-retry", height: 70) {
            MacNotice(
                title: "Could not reach the cloud.",
                advice: nil,
                error: DisplayError(
                    line: "Could not reach the cloud.",
                    detail: "connection refused"
                ),
                action: ErrorAction(perform: {})
            )
        },
        notice(
            "mac.component.notice.danger-actions-below-long-text",
            height: 120
        ) {
            MacNotice(
                title:
                    "Could not reach the cloud. Your changes stay on this device and go up once it is back; nothing is lost in the meantime.",
                advice: nil,
                error: DisplayError(
                    line: "Could not reach the cloud.",
                    detail: "connection refused"
                ),
                action: ErrorAction(perform: {}),
                buttons: .below
            )
        },
        notice("mac.component.notice.danger-title-and-subtitle", height: 90) {
            MacNotice(
                title: "bae could not open your library",
                advice: "Check your connection, then quit and reopen bae.",
                error: nil
            )
        },
        notice("mac.component.notice.warning-quiet-actions-below", height: 90) {
            MacNotice(
                title: "Dropbox signed this device out.",
                advice: nil,
                error: nil,
                tone: .warning,
                look: .quiet,
                action: ErrorAction("Open Sync Settings", perform: {}),
                buttons: .below,
                onDismiss: {}
            )
        },
        notice("mac.component.notice.danger-quiet-with", height: 70) {
            MacNotice(
                title: "Could not close the library",
                advice: nil,
                error: nil,
                look: .quiet,
                onDismiss: {}
            )
        },
        notice("mac.component.notice.danger-centred-a-failed-view", height: 130)
        {
            MacNotice(
                title: "Could not read your library",
                advice: nil,
                error: DisplayError(
                    line: "Could not read your library",
                    detail: "database error"
                ),
                look: .centered,
                action: ErrorAction(perform: {})
            )
        },
    ]

    /// One notice, the state `id` names.
    private static func notice(
        _ id: String,
        height: CGFloat,
        notice: @escaping @MainActor () -> MacNotice
    ) -> ShotScene {
        component(id, height: height) {
            notice().frame(width: 380, alignment: .leading)
        }
    }
}
