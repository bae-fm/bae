import AppKit
import BaeKit
import SwiftUI

/// Both seek bars draw their entire timeline from their position stream,
/// which the native view reads each display frame. SwiftUI hands it the
/// stream and updates appearance, preferences, and callbacks only.
struct SeekBarRepresentable: NSViewRepresentable {
    @Environment(\.accentChoice)
    private var accent
    @Environment(\.colorScheme)
    private var colorScheme

    /// `nil` while nothing plays.
    let source: SeekBarSource?
    let showRemainingTime: Bool
    let onSeek: (Double) -> Void
    let onToggleRemainingTime: (() -> Void)?

    func makeNSView(context _: Context) -> SeekBarNSView {
        let view = SeekBarNSView(
            showsRemainingTimeToggle: onToggleRemainingTime != nil
        )
        apply(to: view)
        return view
    }

    func updateNSView(_ view: SeekBarNSView, context _: Context) {
        apply(to: view)
    }

    static func dismantleNSView(_ view: SeekBarNSView, coordinator _: ()) {
        view.stopFollowing()
    }

    private func apply(to view: SeekBarNSView) {
        view.accent = NSColor(accent.color(in: colorScheme))
        view.onSeek = onSeek
        view.onToggleRemainingTime = onToggleRemainingTime
        view.showRemainingTime = showRemainingTime
        view.follow(source)
    }
}
