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
                MacButton(kind: .borderless, action: {}) {
                    Image(systemName: "doc.on.doc")
                }
                .accessibilityLabel("Copy")
                MacButton(kind: .borderless, action: {}) {
                    Image(systemName: "checkmark")
                }
                .accessibilityLabel("Copied")
            }
        },
    ]
}
