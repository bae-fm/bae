import BaeKit
import SwiftUI

@testable import bae

/// One named UI scene to capture for the screenshot gallery: a stable id shared
/// across platforms by convention, the point size to render at, and a builder
/// for the composed production view. The builder returns the same
/// `PreviewScenes` composition its `#Preview` renders — capture and preview
/// share one path.
@MainActor
struct ShotScene {
    let id: String
    let size: CGSize
    let makeView: () -> AnyView

    /// A current-generation phone's logical size (iPhone 16 portrait). Rendered
    /// offscreen at this size regardless of the booted simulator, so the output
    /// is the same on any device the harness happens to run on.
    static let phoneSize = CGSize(width: 393, height: 852)

    /// Every iOS scene, in gallery order. Only scenes with a real staging are
    /// present; `welcome-restore` has no iOS equivalent and is deliberately
    /// absent rather than faked.
    static let all: [ShotScene] = [
        ShotScene(id: "appearance", size: phoneSize) {
            AnyView(
                Form { AppearanceControls() }.scrollContentBackground(.hidden).windowBackground()
            )
        },
        ShotScene(id: "welcome", size: phoneSize) {
            AnyView(PreviewScenes.welcome())
        },
        ShotScene(id: "library-grid", size: phoneSize) {
            AnyView(PreviewScenes.libraryGrid())
        },
        ShotScene(id: "album-detail", size: phoneSize) {
            AnyView(PreviewScenes.albumDetail())
        },
    ]
}
