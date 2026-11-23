import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacAlert`, one shot for each row of its preview.
    static let macAlertScenes: [ShotScene] = [
        component(
            "mac.component.alert.default",
            width: 600,
            height: 1016
        ) {
            VStack(spacing: ThemeSpace.group) {
                alertOver(
                    height: 240,
                    MacAlertWords(
                        Text(verbatim: "Remove library from this Mac?"),
                        message: Text(
                            verbatim:
                                "The library will remain in the cloud and on your other devices."
                        )
                    ),
                    [
                        MacAlertButton(
                            Text(verbatim: "Remove"),
                            role: .destructive
                        ),
                        MacAlertButton(Text(verbatim: "Cancel"), role: .cancel),
                    ]
                )
                alertOver(
                    height: 240,
                    MacAlertWords(
                        Text(verbatim: "Could not remove Home music"),
                        message: Text(
                            verbatim:
                                "Could not reach the cloud. Check your connection and try again."
                        )
                    ),
                    [
                        MacAlertButton(Text(verbatim: "Retry")),
                        MacAlertButton(Text(verbatim: "Cancel"), role: .cancel),
                    ]
                )
                alertOver(
                    height: 260,
                    MacAlertWords(
                        Text(verbatim: "Unpin Low Tide Hours?"),
                        message: Text(
                            verbatim:
                                "Its files will be removed from this Mac now. It stays in the cloud."
                        ),
                        suppression: .init(
                            Text(verbatim: "Do not show this again"),
                            isOn: .constant(false)
                        )
                    ),
                    [
                        MacAlertButton(Text(verbatim: "Unpin")),
                        MacAlertButton(Text(verbatim: "Cancel"), role: .cancel),
                    ]
                )
                // A label wider than half the row stacks the two buttons.
                alertOver(
                    height: 240,
                    MacAlertWords(
                        Text(verbatim: "bae will remove the device"),
                        message: Text(
                            verbatim:
                                "Dropbox: Remove bae’s access in the provider’s account settings."
                        )
                    ),
                    [
                        MacAlertButton(Text(verbatim: "I cut off this device")),
                        MacAlertButton(
                            Text(verbatim: "Not yet"),
                            role: .cancel
                        ),
                    ]
                )
            }
        }
    ]

    /// An alert hung where `OverWindow` hangs one, under the top of a window
    /// background `height` tall.
    private static func alertOver(
        height: CGFloat,
        _ words: MacAlertWords,
        _ buttons: [MacAlertButton]
    ) -> some View {
        ZStack(alignment: .top) {
            Rectangle().fill(Theme.background)
            MacAlert(words, buttons: buttons)
                .sheetFace()
                .padding(.top, OverWindow.hangs)
        }
        .frame(height: height)
    }
}
