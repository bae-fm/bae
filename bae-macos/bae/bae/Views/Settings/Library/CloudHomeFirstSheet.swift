import BaeKit
import SwiftUI

/// The sheet that says a device or a person joins only through a cloud
/// home: Cancel, or Set up a cloud home…, which opens the setup.
struct CloudHomeFirstSheet: View {
    /// What the person asked for.
    enum Reason {
        case addDevice
        case invite
    }

    let reason: Reason
    let onCancel: () -> Void
    let onSetUp: () -> Void

    var body: some View {
        MacSheet(title, message: message, width: 420) {
            EmptyView()
        } footer: {
            MacSheetFooter {
                MacButton("Cancel", action: onCancel)
                    .keyboardShortcut(.cancelAction)
                MacButton(
                    "Set up a cloud home…",
                    kind: .primary,
                    action: onSetUp
                )
                .keyboardShortcut(.defaultAction)
            }
        }
    }

    private var title: LocalizedStringKey {
        switch reason {
        case .addDevice: "Add a device"
        case .invite: "Invite someone"
        }
    }

    private var message: LocalizedStringKey {
        switch reason {
        case .addDevice:
            "To add a device, set up a cloud home first. Your library and music stay on this Mac until you do."
        case .invite:
            "To invite someone, set up a cloud home first. Your library and music stay on this Mac until you do."
        }
    }
}
