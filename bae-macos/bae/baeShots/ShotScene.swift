import BaeKit
import SwiftUI

@testable import bae

/// One screen of the app to shoot: the name of the design board it is
/// compared with (`mac.<area>.<screen>[.<state>]`), the point size to render
/// at, and a builder for the composed production view and what it waits for. The
/// builder returns the same `PreviewScenes` composition its `#Preview`
/// renders — capture and preview share one path.
@MainActor
struct ShotScene {
    let id: String
    /// The view's size, in points; a scene shown in its window adds the
    /// window's title bar.
    let size: CGSize
    var frame: ShotFrame = .content
    let stage: () -> ShotStage

    /// Every macOS scene, one for each board it is shot for. A scene is
    /// present only when it has a real staging; a missing scene is a
    /// deliberate omission, never a swallowed failure.
    static let all: [ShotScene] =
        welcomeScenes + componentScenes + syncSettingsScenes
        + storageSettingsScenes + alertScenes + importListScenes
        + adminChangeScenes + menuScenes
}

/// A scene's view, built with its fixture, and whether what the view shows
/// has arrived: the reads it makes off the main thread answered and taken
/// up, an image decoded. The scene knows which reads it needs, so it says
/// when they are done; a scene whose views read nothing off the main thread
/// is ready at once.
@MainActor
struct ShotStage {
    let view: AnyView
    /// Read under observation tracking, so it holds only what a store
    /// publishes.
    let isReady: @MainActor () -> Bool

    init(
        _ view: some View,
        isReady: @escaping @MainActor () -> Bool = { true }
    ) {
        self.view = AnyView(view)
        self.isReady = isReady
    }

    /// `view`, whose art comes from `images`, ready once the store has
    /// handed back every image asked of it, decoded or failed, and
    /// `isReady` holds. The store hands an image back a main-actor step
    /// before the view that asked shows it, and a view asks a step after it
    /// is first drawn; the tree's coming to rest spans both.
    init(
        _ view: some View,
        images: ImageStore,
        isReady: @escaping @MainActor () -> Bool = { true }
    ) {
        self.init(view) { images.unansweredImages == 0 && isReady() }
    }
}

/// What a shot takes in: the scene's view alone, or the window it is shown
/// in, title bar and toolbar included, for a scene whose toolbar is part of
/// what it shows.
///
/// A window takes the focus, so only a run given `--windows` takes the shot.
/// bae-shots reads which scenes those are from the source, so a scene shown
/// in its window is written with the id as a string literal of its board
/// name and `frame: .window(title: …)` among the arguments of its
/// `ShotScene(` call; a window frame the tool cannot tie to such an id
/// fails its reading. A run
/// without `--windows` reports these shots as not taken, never as missing.
enum ShotFrame {
    case content
    case window(title: String, titleBar: TitleBar = .shown)

    /// How a window draws its title bar: as a titled window does, or, as
    /// the Settings window does, transparent and without its title, the
    /// content reaching under it and the window's buttons sitting over it.
    enum TitleBar {
        case shown
        case hidden
    }
}
