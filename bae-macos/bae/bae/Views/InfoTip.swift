import SwiftUI

/// A "?" icon that shows an info popover on hover.
/// The popover stays open while the cursor is over either the icon or the popover itself.
struct InfoTip: View {
    let text: String
    var learnMoreURL: URL?
    var width: CGFloat = 260
    var arrowEdge: Edge = .top

    @State
    private var isShowing = false
    @State
    private var hoverTask: DispatchWorkItem?

    var body: some View {
        Image(systemName: "questionmark.circle")
            .font(.callout)
            .foregroundStyle(.tertiary)
            .onHover { hovering in
                scheduleHover(show: hovering)
            }
            .popover(isPresented: $isShowing, arrowEdge: arrowEdge) {
                VStack(alignment: .leading, spacing: 6) {
                    Text(text)
                        .font(.callout)
                    if let url = learnMoreURL {
                        Link("Learn more", destination: url)
                            .font(.callout)
                    }
                }
                .padding(10)
                .frame(width: width)
                .onHover { hovering in
                    if !hovering {
                        scheduleHover(show: false)
                    }
                    else {
                        hoverTask?.cancel()
                    }
                }
            }
    }

    private func scheduleHover(show: Bool) {
        hoverTask?.cancel()
        let task = DispatchWorkItem {
            isShowing = show
        }
        hoverTask = task
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.3, execute: task)
    }
}
