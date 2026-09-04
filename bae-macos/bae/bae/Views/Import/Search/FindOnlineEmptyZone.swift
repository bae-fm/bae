import SwiftUI

/// The result area when there is nothing to list: the zone's name in its
/// corner and, in the middle, one line saying what happened with the one thing
/// to do about it. The form below stays where it is, so the action points at
/// it rather than repeating its controls.
struct FindOnlineEmptyZone<Content: View>: View {
    @ViewBuilder
    let content: Content

    var body: some View {
        ZStack(alignment: .topLeading) {
            HStack(spacing: 8) {
                content
            }
            .font(.system(size: 13))
            .padding(.horizontal, 18)
            .padding(.vertical, 30)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            FindOnlineCapsLabel("Automatic")
                .padding(.top, 10)
                .padding(.leading, 14)
        }
    }
}

#if DEBUG
    // MARK: - Previews

    #Preview("Nothing found") {
        FindOnlineEmptyZone {
            Text("No matches.")
                .foregroundStyle(.secondary)
            Button("Search instead") {}
                .buttonStyle(.link)
        }
        .frame(width: 620, height: 160)
        .windowBackground()
    }

    #Preview("Not looked up") {
        FindOnlineEmptyZone {
            IdentifyAutomaticallyButton(action: {})
        }
        .frame(width: 620, height: 160)
        .windowBackground()
    }
#endif
