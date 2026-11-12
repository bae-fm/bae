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
    private static let hangs: CGFloat = 36

    /// A sheet or an alert, hung from the top edge, centred.
    @MainActor
    static func sheet(
        _ dialog: some View,
        over window: some View
    ) -> some View {
        ZStack(alignment: .top) {
            window
            dialog
                .background(Color(nsColor: .windowBackgroundColor))
                .clipShape(RoundedRectangle(cornerRadius: 20))
                .shadow(color: .black.opacity(0.3), radius: 15, y: 5)
                .padding(.top, hangs)
        }
    }

    /// An alert, hung from the top edge as the app's overlay hangs it: bae's
    /// own face, 20 points rounded, over the window.
    @MainActor
    static func alert(
        _ alert: (MacAlertWords, [MacAlertButton]),
        over window: some View
    ) -> some View {
        ZStack(alignment: .top) {
            window
            MacAlertView(alert.0, buttons: alert.1)
                .elevatedFace()
                .padding(.top, hangs)
        }
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
        ZStack(alignment: .topTrailing) {
            window
            content
                .fixedSize(horizontal: false, vertical: true)
                .background(Color(nsColor: .windowBackgroundColor))
                .clipShape(RoundedRectangle(cornerRadius: 10))
                .overlay(alignment: .topTrailing) {
                    Rectangle()
                        .fill(Color(nsColor: .windowBackgroundColor))
                        .frame(width: 14, height: 14)
                        .rotationEffect(.degrees(45))
                        .offset(x: -40, y: -7)
                        .accessibilityHidden(true)
                }
                .shadow(color: .black.opacity(0.3), radius: 15, y: 5)
                .padding(.top, top)
                .padding(.trailing, trailing)
        }
    }
}

/// The Settings window as the app draws it, `SettingsWindowLayout`, on the
/// pane `tab`, with `page` as that pane, and the sync state the sidebar's
/// badges read.
struct SettingsWindowShot<Page: View>: View {
    let shown: BridgeSyncShown
    let pending: [BridgePendingRecord]
    let undoneSetup: BridgeUndoneStorageSetup?
    let page: Page

    /// The pane the window is on, as the person left it.
    @State
    private var selection: SettingsTab

    init(
        tab: SettingsTab = .library,
        shown: BridgeSyncShown = .previewSynced,
        pending: [BridgePendingRecord] = [],
        undoneSetup: BridgeUndoneStorageSetup? = nil,
        page: Page
    ) {
        self.shown = shown
        self.pending = pending
        self.undoneSetup = undoneSetup
        self.page = page
        _selection = State(initialValue: tab)
    }

    var body: some View {
        SettingsWindowLayout(selection: $selection) { page }
            .environment(DownloadStore())
            .environment(OutboxStore(uploadsPaused: false))
            .environment(SubsonicStore())
            .environment(
                SyncStatusStore(
                    snapshot: BridgeSyncStatusSnapshot(
                        shown: shown,
                        pending: pending,
                        setupRunning: false,
                        undoneSetup: undoneSetup
                    )
                )
            )
    }
}
