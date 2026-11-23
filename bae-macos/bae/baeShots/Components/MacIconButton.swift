import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacIconButton`, one shot for each row of its preview.
    static let macIconButtonScenes: [ShotScene] = [
        component("mac.component.icon-button.resting", height: 60) {
            MacIconButton(symbol: "shuffle", label: "Shuffle") {}
        },
        component("mac.component.icon-button.active", height: 60) {
            MacIconButton(symbol: "shuffle", label: "Shuffle", active: true) {}
        },
        component("mac.component.icon-button.faint", height: 60) {
            MacIconButton(symbol: "shuffle", label: "Shuffle", faint: true) {}
        },
        component("mac.component.icon-button.small", height: 60) {
            MacIconButton(
                symbol: "shuffle",
                label: "Shuffle",
                iconSize: .medium,
                hitSize: BoxedIconButtonStyle.Size.small.side
            ) {}
        },
        component("mac.component.icon-button.boxed", height: 60) {
            MacIconButton(
                symbol: "shuffle",
                label: "Shuffle",
                iconSize: .large,
                look: .boxed(small: false)
            ) {}
        },
        component("mac.component.icon-button.overlay", height: 60) {
            MacIconButton(symbol: "shuffle", label: "Shuffle") {}
                .foregroundStyle(Color.white)
                .background(Color.black.opacity(0.5))
        },
        component("mac.component.icon-button.off", height: 60) {
            MacIconButton(symbol: "shuffle", label: "Shuffle") {}
                .disabled(true)
        },
        component("mac.component.icon-button.menu", height: 60) {
            MacIconButton.menu(symbol: "shuffle", label: "Shuffle") {
                MacMenuItem("Shuffle") {}
            }
        },
    ]
}
