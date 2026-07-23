import SwiftUI

/// Popover content shown when hovering over a disabled Discogs segment.
struct DiscogsKeyPopover: View {
    @Binding
    var isPresented: Bool
    @Binding
    var hoverTask: DispatchWorkItem?
    let onOpenSettings: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(
                "[Discogs](https://www.discogs.com) is a music database with detailed release info — labels, catalog numbers, pressing variants, and more."
            )
            .font(.callout)
            Text(
                "To search Discogs, [get your free API key](https://www.discogs.com/settings/developers) and add it in settings."
            )
            .font(.callout)
            Button("Open Settings") {
                isPresented = false
                onOpenSettings()
            }
            .font(.callout)
        }
        .padding(10)
        .frame(width: 280)
        .onHover { hovering in
            hoverTask?.cancel()
            if !hovering {
                let task = DispatchWorkItem { isPresented = false }
                hoverTask = task
                DispatchQueue.main.asyncAfter(
                    deadline: .now() + 0.3,
                    execute: task
                )
            }
        }
    }
}
