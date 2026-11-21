import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacMoreMenu`, one shot for each row of its preview.
    static let macMoreMenuScenes: [ShotScene] = [
        // `MacMoreMenu` open: its menu drawn in place 4 points under the button,
        // the item the pointer is on highlighted.
        component("mac.component.more-menu.open", height: 150) {
            ZStack(alignment: .topTrailing) {
                MacMoreMenu {
                    MacMenuItem("Details…") {}
                }
                MacMenuInPlace(
                    {
                        MacMenuItem("Details…") {}
                        MacMenuItem("Rename…") {}
                        MacMenuItem.divider
                        MacMenuItem("Remove from this Mac…") {}
                    },
                    highlighted: 1
                )
                .padding(.top, ThemeSize.hitTarget + 4)
            }
            .frame(width: 380, alignment: .topTrailing)
        },
        // `MacMoreMenu` off, boxed and over a cover, at the end of a row.
        component("mac.component.more-menu.off", width: 580, height: 60) {
            MacMoreMenu {
                MacMenuItem("Details…") {}
            }
            .disabled(true)
            .frame(maxWidth: .infinity, alignment: .trailing)
        },
        component("mac.component.more-menu.boxed", width: 580, height: 60) {
            MacMoreMenu(look: .boxed) {
                MacMenuItem("Details…") {}
            }
            .frame(maxWidth: .infinity, alignment: .trailing)
        },
        component("mac.component.more-menu.overlay", width: 580, height: 60) {
            MacMoreMenu(look: .overlay) {
                MacMenuItem("Details…") {}
            }
            .frame(maxWidth: .infinity, alignment: .trailing)
        },
        // `MacMoreMenu`, closed.
        component("mac.component.more-menu.closed", height: 60) {
            MacMoreMenu {
                MacMenuItem("Details…") {}
            }
        },
    ]
}
