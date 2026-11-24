import AppKit
import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacSavePanelFormat` closed, on FLAC. Its open rows are the design's
    /// only: AppKit draws the menu in a window of its own, which a shot
    /// can't open.
    static let macSavePanelFormatScenes: [ShotScene] = [
        component(
            "mac.component.save-panel-format.closed",
            width: wideColumn,
            height: 50
        ) {
            SavePanelFormatShot(formats: ["FLAC", "MP3", "Phone"])
                .frame(width: 250, height: 34)
                .frame(maxWidth: .infinity)
        }
    ]

    /// The Save as panel's format line with `formats` in its pop-up.
    private struct SavePanelFormatShot: NSViewRepresentable {
        let formats: [String]

        func makeNSView(context _: Context) -> NSView {
            let popup = MacSavePanelFormatPopUp(
                frame: MacSavePanelFormat.popupFrame,
                pullsDown: false
            )
            popup.addItems(withTitles: formats)
            return MacSavePanelFormat(popup: popup)
        }

        func updateNSView(_: NSView, context _: Context) {}
    }
}
