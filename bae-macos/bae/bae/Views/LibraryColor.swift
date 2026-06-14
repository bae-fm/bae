import SwiftUI

/// Fixed palette of named colors the sidebar lets a user pin to a
/// library row. Stable raw names so the on-disk preference survives
/// future palette additions.
enum LibraryColor: String, CaseIterable {
    case red, orange, yellow, green, blue, purple, pink

    var label: String {
        switch self {
        case .red: "Red"
        case .orange: "Orange"
        case .yellow: "Yellow"
        case .green: "Green"
        case .blue: "Blue"
        case .purple: "Purple"
        case .pink: "Pink"
        }
    }

    var swiftUIColor: Color {
        switch self {
        case .red: .red
        case .orange: .orange
        case .yellow: .yellow
        case .green: .green
        case .blue: .blue
        case .purple: .purple
        case .pink: .pink
        }
    }
}
