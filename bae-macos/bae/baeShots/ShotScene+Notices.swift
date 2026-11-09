import AppKit
import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The error notice's looks and what its Details opens, each beside its
    /// canvas board.
    static let noticeScenes: [ShotScene] = [
        ShotScene(
            id: "notice-details-alert",
            size: CGSize(width: 340, height: 320)
        ) {
            ShotStage(
                ErrorDetailsAlertShot(
                    details: ErrorDetails(
                        title: "Couldn’t load sign-in settings",
                        advice:
                            "This copy of bae can’t read its Dropbox and OneDrive sign-in settings, so it can’t sign in to them. Reinstall bae.",
                        error: DisplayError(
                            line: "The sign-in settings file is missing.",
                            detail: "OAuthConfig: missing client id"
                        )
                    )
                )
            )
        }
    ]
}

/// The alert a notice's Details opens, as AppKit lays it out: its window's
/// content drawn into an image, since a modal alert can't be captured
/// offscreen.
private struct ErrorDetailsAlertShot: View {
    let details: ErrorDetails

    @Environment(\.colorScheme)
    private var colorScheme

    var body: some View {
        let alert = ErrorDetailsAlert.makeAlert(details)
        let window = alert.window
        window.appearance = NSAppearance(
            named: colorScheme == .dark ? .darkAqua : .aqua
        )
        alert.layout()
        guard let view = window.contentView,
            let rep = view.bitmapImageRepForCachingDisplay(in: view.bounds)
        else {
            preconditionFailure("An alert's window has content to draw")
        }
        let bounds = view.bounds
        view.cacheDisplay(in: bounds, to: rep)
        let image = NSImage(size: bounds.size)
        image.addRepresentation(rep)
        return Image(nsImage: image)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}
