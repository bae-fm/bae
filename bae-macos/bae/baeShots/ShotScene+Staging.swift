import AppKit
import SwiftUI

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

    /// An alert: the image AppKit draws it as, on the opaque panel a modal
    /// alert has.
    @MainActor
    static func alert(
        _ alert: @escaping () -> NSAlert,
        over window: some View
    ) -> some View {
        sheet(
            NSAlertShot(alert: alert).fixedSize(),
            over: window
        )
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
