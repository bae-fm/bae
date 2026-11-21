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
            MacIconButton(symbol: "repeat", label: "Repeat", active: true) {}
        },
        component("mac.component.icon-button.faint", height: 60) {
            MacIconButton(symbol: "shuffle", label: "Shuffle") {}
                .foregroundStyle(Theme.textTertiary)
        },
        component("mac.component.icon-button.small", height: 60) {
            MacIconButton(symbol: "shuffle", label: "Shuffle") {}
                .controlSize(.small)
        },
        component("mac.component.icon-button.boxed", height: 60) {
            MacIconButton(symbol: "shuffle", label: "Shuffle") {}
                .padding(ThemeSpace.related)
                .overlay(
                    RoundedRectangle(cornerRadius: ThemeRadius.bar)
                        .stroke(Theme.hairline)
                )
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
            MacButton(
                kind: .plain,
                menu: { MacMenuItem("Shuffle") {} }
            ) {
                Image(systemName: "shuffle")
            }
            .accessibilityLabel("Shuffle")
        },
    ]
}
