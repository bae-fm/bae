import BaeKit
import SwiftUI

/// The welcome window's actions: Create new library… as the default action,
/// then Scan code and Enter code side by side.
struct MacWelcomeActions: View {
    let disabled: Bool
    let onCreate: () -> Void
    let onScanCode: () -> Void
    let onEnterCode: () -> Void

    /// The width the buttons share.
    private static let width: CGFloat = 240

    var body: some View {
        VStack(spacing: ThemeSpace.related + ThemeSpace.line) {
            MacButton(kind: .primary, action: onCreate) {
                Text("Create new library…")
                    .frame(maxWidth: .infinity)
            }
            .fillsWidth()
            .disabled(disabled)
            .keyboardShortcut(.defaultAction)
            HStack(spacing: ThemeSpace.related) {
                MacButton(action: onScanCode) {
                    Text("Scan code").frame(maxWidth: .infinity)
                }
                .fillsWidth()
                MacButton(action: onEnterCode) {
                    Text("Enter code").frame(maxWidth: .infinity)
                }
                .fillsWidth()
            }
            .disabled(disabled)
        }
        .controlSize(.large)
        .frame(width: Self.width)
    }
}
