import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacPopUpButton` at rest, off, and open: the menu drawn in place over
    /// the button, the chosen item on the button's line.
    static let macPopUpButtonScenes: [ShotScene] = [
        component("mac.component.pop-up-button.resting", height: 50) {
            popUp()
        },
        component("mac.component.pop-up-button.off", height: 50) {
            popUp().disabled(true)
        },
        component("mac.component.pop-up-button.open", height: 130) {
            let options: [MacPopUpEntry<String>] = [
                .option(MacPopUpOption(verbatim: "Off", value: "Off")),
                .option(MacPopUpOption(verbatim: "Only me", value: "Only me")),
                .option(
                    MacPopUpOption(verbatim: "Everyone", value: "Everyone")
                ),
            ]
            let menu = MacMenu(options, selection: "Only me", select: { _ in })
            ZStack(alignment: .topLeading) {
                popUp()
                    .padding(.leading, 40)
                    .padding(.top, 40)
                MacMenuInPlace(
                    { menu.entries },
                    highlighted: menu.chosen,
                    isPopUp: true
                )
                // The menu's left edge is 31 points left of the button's,
                // and its chosen line is on the button's.
                .padding(.leading, 40 - 31)
                .padding(.top, 40 - 6 - 24)
            }
        },
    ]

    private static func popUp() -> some View {
        MacPopUpButton("Share with", selection: .constant("Only me")) {
            MacPopUpOption(verbatim: "Off", value: "Off")
            MacPopUpOption(verbatim: "Only me", value: "Only me")
            MacPopUpOption(verbatim: "Everyone", value: "Everyone")
        }
    }
}
