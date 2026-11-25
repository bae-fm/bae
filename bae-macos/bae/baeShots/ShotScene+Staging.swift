import AppKit
import BaeKit
import SwiftUI

@testable import bae

/// A dialog laid over the window it opens on, as the boards draw it: the
/// window as it stands, and the alert, sheet or popover on top. The app
/// shows these in AppKit's own windows, which a capture of the view can't
/// reach, so the scene lays the dialog's content over the window itself.
enum OverWindow {
    /// How far below the window's top a sheet or an alert hangs (the boards'
    /// `margin-top` on `bae-m-sheet` and `bae-m-alert`).
    static let hangs: CGFloat = 36

    /// A sheet, hung from the top edge, centred, over the scrim that dims the
    /// whole window. The app's sheet is a window of its own, which dims the
    /// window it hangs from.
    @MainActor
    static func sheet(
        _ dialog: some View,
        over window: some View
    ) -> some View {
        ZStack(alignment: .top) {
            window
            MacScrim()
            dialog
                .sheetFace()
                .padding(.top, hangs)
                // The app's sheet is a window of its own, read after the
                // window it hangs from, not among what it covers.
                .accessibilitySortPriority(-1)
        }
    }

    /// A sheet presented as the app presents one: a real sheet attached to
    /// the window, over the dimming macOS draws behind it. The scene is shot
    /// in its window, and its stage waits for the sheet (`awaitingSheet`).
    @MainActor
    static func attachedSheet(
        _ dialog: some View,
        over window: some View
    ) -> some View {
        window.sheet(isPresented: .constant(true)) {
            dialog.background { DisableSheetAnimation() }
        }
    }

    /// A sheet centred over the scrim that dims the whole window, as macOS
    /// places a sheet over the welcome window.
    @MainActor
    static func centredSheet(
        _ dialog: some View,
        over window: some View
    ) -> some View {
        ZStack {
            window
            MacScrim()
            dialog
                .sheetFace()
                // The app's sheet is a window of its own, read after the
                // window it covers.
                .accessibilitySortPriority(-1)
        }
    }

    /// An alert, raised over `window` as the app raises it: `.macAlert`'s
    /// own panel, a child window of the window. The shot captures the window
    /// and the panel from the window server, so the scene presents the alert
    /// in a real window.
    @MainActor
    static func alert(
        _ alert: (MacAlertWords, [MacAlertButton]),
        over window: some View
    ) -> some View {
        window.macAlert(isPresented: .constant(true)) { alert }
    }

    /// A popover, its top edge `top` points below the window's top and its
    /// right edge `trailing` points in from the window's right, with the
    /// arrow on its top edge pointing at what opened it.
    @MainActor
    static func popover(
        _ content: some View,
        top: CGFloat,
        trailing: CGFloat,
        over window: some View
    ) -> some View {
        placedFromTop(
            content,
            distance: top,
            trailing: trailing,
            over: window
        )
    }

    /// A popover that opens upward from a control at the window's bottom,
    /// drawn as the app draws one (`MacPopoverInPlace`): its body's bottom
    /// edge `bottom` points above the window's bottom, its right edge
    /// `trailing` points in from the window's right, and its arrow, which
    /// points down at the control, `arrowFromTrailing` points in from the
    /// body's right edge, as the boards place it.
    @MainActor
    static func popover(
        _ content: some View,
        bottom: CGFloat,
        trailing: CGFloat,
        arrowFromTrailing: CGFloat,
        over window: some View
    ) -> some View {
        ZStack(alignment: .bottomTrailing) {
            window
            PlacedPopover(
                content: content,
                arrowSide: .bottom,
                arrowFromTrailing: arrowFromTrailing
            )
            // The face holds its arrow under the body and a room for its
            // shadow around it.
            .padding(.bottom, bottom - popoverArrowLength - popoverShadowRoom)
            .padding(.trailing, trailing - popoverShadowRoom)
            // The app's popover is a window of its own, which the boards
            // read where it sits, before the player bar under it.
            .accessibilitySortPriority(0)
        }
    }

    /// `content` drawn as a popover over `window`, `distance` points from
    /// the window's top and `trailing` points in from its right, with the
    /// arrow on its top edge.
    @MainActor
    private static func placedFromTop(
        _ content: some View,
        distance: CGFloat,
        trailing: CGFloat,
        over window: some View
    ) -> some View {
        // An overlay, so the popover's face and the room round it for its
        // shadow never change the window's own size.
        window.overlay(alignment: .topTrailing) {
            PlacedPopover(
                content: content,
                arrowSide: .top,
                // The boards' arrow is 14 points wide, 40 from the body's
                // right edge, so its middle is 47 in.
                arrowFromTrailing: 47
            )
            // The face holds its arrow over the body and a room for its
            // shadow around it.
            .padding(.top, distance - popoverArrowLength - popoverShadowRoom)
            .padding(.trailing, trailing - popoverShadowRoom)
            // The app's popover is a window of its own. The boards read
            // one hung from the top after the window it opens from, not
            // among the rows it covers.
            .accessibilitySortPriority(-1)
        }
    }
}

extension ShotStage {
    /// This stage, drawn once the window holds its attached sheet.
    func awaitingSheet() -> ShotStage {
        ShotStage(view, isReady: isReady) { host in
            isDrawn(host) && host.window?.attachedSheet != nil
        }
    }
}

/// Turns off AppKit's sheet animation as SwiftUI puts the content in its window.
struct DisableSheetAnimation: NSViewRepresentable {
    func makeNSView(context: Context) -> NSView { WindowAnchor() }

    func updateNSView(_: NSView, context: Context) {}

    private final class WindowAnchor: NSView {
        override func viewWillMove(toWindow newWindow: NSWindow?) {
            super.viewWillMove(toWindow: newWindow)
            guard let newWindow else { return }
            precondition(
                !newWindow.isVisible || newWindow.animationBehavior == .none,
                "The sheet appeared before the shot disabled its animation."
            )
            newWindow.animationBehavior = .none
        }
    }
}

/// The length of a popover's arrow and the room around its face for the
/// shadow, which BaeKit's `MacPopoverFace` draws.
let popoverArrowLength = MacPopoverMetrics.arrowLength
let popoverShadowRoom = MacPopoverMetrics.shadowRoom

/// `content` in the popover face the app draws, its arrow on the `arrowSide`
/// edge `arrowFromTrailing` points in from the right edge.
private struct PlacedPopover<Content: View>: View {
    let content: Content
    let arrowSide: Edge
    let arrowFromTrailing: CGFloat

    @State
    private var width: CGFloat = 0

    var body: some View {
        MacPopoverInPlace(
            arrowSide: arrowSide,
            arrowOffset: width - arrowFromTrailing
        ) {
            content
                .fixedSize(horizontal: false, vertical: true)
                .onGeometryChange(for: CGFloat.self, of: { $0.size.width }) {
                    width = $0
                }
        }
    }
}

/// The Settings window as the app draws it, `SettingsWindowLayout`, on the
/// pane `tab`, with `page` as that pane, and the sync state the sidebar's
/// badges read.
struct SettingsWindowShot<Page: View>: View {
    let shown: BridgeSyncShown
    let undoneSetup: BridgeUndoneStorageSetup?
    let page: Page

    /// The pane the window is on, as the person left it.
    @State
    private var navigation: SettingsNavigation

    init(
        tab: SettingsTab = .library,
        shown: BridgeSyncShown = .fixtureSynced,
        undoneSetup: BridgeUndoneStorageSetup? = nil,
        page: Page
    ) {
        self.shown = shown
        self.undoneSetup = undoneSetup
        self.page = page
        let navigation = SettingsNavigation()
        navigation.selectedTab = tab
        _navigation = State(initialValue: navigation)
    }

    var body: some View {
        SettingsWindowLayout(navigation: navigation) { page }
            .environment(DownloadStore())
            .environment(OutboxStore(uploadsPaused: false))
            .environment(SubsonicStore())
            .environment(
                SyncStatusStore(
                    snapshot: BridgeSyncStatusSnapshot(
                        shown: shown,
                        setupRunning: false,
                        hasCloudHome: true,
                        undoneSetup: undoneSetup
                    )
                )
            )
    }
}
