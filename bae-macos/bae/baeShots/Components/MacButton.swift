import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacButton`: the sizes, the kinds, off, and the icon button.
    static let macButtonScenes: [ShotScene] = [
        component("mac.component.button.sizes", height: 60) {
            HStack(spacing: 8) {
                MacButton("Small", kind: .primary) {}
                    .controlSize(.small)
                MacButton("Regular", kind: .primary) {}
                MacButton("Large", kind: .primary) {}
                    .controlSize(.large)
            }
        },
        component("mac.component.button.primary", height: 50) {
            MacButton("Set up", kind: .primary) {}
        },
        component("mac.component.button.secondary", height: 50) {
            MacButton("Set up") {}
        },
        component("mac.component.button.borderless", height: 50) {
            MacButton("Set up", kind: .borderless) {}
        },
        component("mac.component.button.borderless-neutral", height: 50) {
            MacButton("Cancel", kind: .borderless) {}
                .neutral()
        },
        component("mac.component.button.off", height: 50) {
            MacButton("Set up", kind: .primary) {}
                .disabled(true)
        },
        component("mac.component.button.icon", height: 50) {
            HStack(spacing: 8) {
                MacButton(kind: .icon, action: {}) {
                    Image(systemName: "doc.on.doc")
                }
                .accessibilityLabel("Copy")
                MacButton(kind: .icon, action: {}) {
                    Image(systemName: "checkmark")
                }
                .accessibilityLabel("Copied")
            }
        },
        component("mac.component.button.destructive", height: 50) {
            MacButton("Set up") {}
                .tint(Theme.danger)
        },
        component("mac.component.button.menu", height: 50) {
            MacButton(menu: { MacMenuItem("Details…") {} }) {
                Text("Set up")
            }
        },
        component("mac.component.button.muted", height: 50) {
            MacButton("Set up") {}
                .muted()
        },
        component("mac.component.button.size-alert", width: 842, height: 50) {
            MacButton(kind: .secondary, action: {}) {
                Text("Set up").frame(maxWidth: .infinity)
            }
            .controlSize(.large)
        },
        component("mac.component.button.tinted", height: 50) {
            MacButton("Set up") {}
                .tinted()
        },
    ]
}
