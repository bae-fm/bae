import BaeKit
import SwiftUI

/// The main window's chrome around whatever screen it shows — the minimum
/// window size, the themed background stretched to fill however large the
/// window grows, and the bottom line for a library load error. BaeApp renders
/// it live around the shell and the bootstrap screens; the welcome previews
/// render the same composition, so the canvas shows a screen exactly as the
/// window does and the two cannot drift apart.
struct MainWindowChrome<Content: View>: View {
    let loadError: String?
    @ViewBuilder
    let content: Content

    var body: some View {
        content
            .frame(minWidth: 900, minHeight: 600)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .windowBackground()
            .overlay(alignment: .bottom) {
                if let loadError {
                    Text(loadError)
                        .foregroundStyle(.red)
                        .padding()
                }
            }
    }
}
