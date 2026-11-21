import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacSegmentedControl`: two choices, three, and off.
    static let macSegmentedControlScenes: [ShotScene] = [
        component("mac.component.segmented-control.two-choices", height: 60) {
            MacSegmentedControl(
                selectedIndex: .constant(1),
                segments: [
                    .init(label: "In person", systemImage: nil),
                    .init(label: "Send", systemImage: nil),
                ]
            )
        },
        component("mac.component.segmented-control.three-choices", height: 60) {
            MacSegmentedControl(
                selectedIndex: .constant(1),
                segments: [
                    .init(label: "Off", systemImage: nil),
                    .init(label: "Only me", systemImage: nil),
                    .init(label: "Everyone", systemImage: nil),
                ]
            )
        },
        component("mac.component.segmented-control.off", height: 60) {
            MacSegmentedControl(
                selectedIndex: .constant(1),
                segments: [
                    .init(label: "Off", systemImage: nil),
                    .init(label: "Only me", systemImage: nil),
                    .init(label: "Everyone", systemImage: nil),
                ],
                off: true
            )
        },
    ]
}
