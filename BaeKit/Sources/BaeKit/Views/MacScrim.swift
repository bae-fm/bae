import SwiftUI

/// The dimming behind a Mac alert, in-window sheet or side pause:
/// `Theme.scrim` across the whole window, title bar included. A view inside
/// the window (`MacModal`, `MacSidePause`) draws it as this view, and a window
/// that sits over another one (`MacAlert`'s panel) draws it through
/// `ScrimWindow`. A system sheet (`MacSheet`) draws none: macOS dims the
/// window it hangs from.
public struct MacScrim: View {
    public init() {}

    public var body: some View {
        Theme.scrim.ignoresSafeArea()
    }
}
