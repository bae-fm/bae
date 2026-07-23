import BaeKit
import SwiftUI

@testable import bae

/// One named UI scene to capture for the screenshot gallery: a stable id
/// shared across platforms by convention, the point size to render at, and a
/// builder for the composed production view. The builder returns the same
/// `PreviewScenes` composition its `#Preview` renders — capture and preview
/// share one path.
@MainActor
struct ShotScene {
    let id: String
    let size: CGSize
    let makeView: () -> AnyView

    /// Every macOS scene, in gallery order. A scene is present only when it has
    /// a real staging; a missing scene is a deliberate omission, never a
    /// swallowed failure.
    static let all: [ShotScene] = [
        ShotScene(id: "welcome", size: WelcomeWindow.size) {
            AnyView(PreviewScenes.welcome())
        },
        ShotScene(id: "welcome-restore", size: WelcomeWindow.size) {
            AnyView(PreviewScenes.welcomeRestore())
        },
        ShotScene(id: "library-grid", size: MainWindow.defaultSize) {
            AnyView(PreviewScenes.libraryGrid())
        },
        ShotScene(
            id: "album-detail",
            size: CGSize(width: 1100, height: 900)
        ) {
            AnyView(PreviewScenes.albumDetail())
        },
    ]
}
