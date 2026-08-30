import SwiftUI

struct SearchSourceToggles: View {
    @Binding
    var musicBrainzSelected: Bool
    @Binding
    var discogsSelected: Bool
    let discogsEnabled: Bool
    @Binding
    var showDiscogsInfo: Bool
    @Binding
    var hoverTask: DispatchWorkItem?

    var body: some View {
        HStack(spacing: 12) {
            Toggle("MusicBrainz", isOn: $musicBrainzSelected)
            ZStack {
                Toggle("Discogs", isOn: availableDiscogsSelection)
                    .disabled(!discogsEnabled)
                if !discogsEnabled {
                    Color.clear
                        .contentShape(Rectangle())
                        .onHover(perform: showDiscogsPopoverDelayed)
                }
            }
        }
        .toggleStyle(.checkbox)
        .controlSize(.small)
    }

    private var availableDiscogsSelection: Binding<Bool> {
        Binding(
            get: { discogsEnabled && discogsSelected },
            set: { discogsSelected = $0 }
        )
    }

    private func showDiscogsPopoverDelayed(_ show: Bool) {
        hoverTask?.cancel()
        let task = DispatchWorkItem {
            showDiscogsInfo = show
        }
        hoverTask = task
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.3, execute: task)
    }
}

#if DEBUG
    #Preview("Search Sources - Discogs Disabled") {
        SearchSourceTogglesPreview()
            .frame(width: 500, height: 100)
            .windowBackground()
    }

    private struct SearchSourceTogglesPreview: View {
        @State
        private var musicBrainzSelected = true
        @State
        private var discogsSelected = true
        @State
        private var showPopover = false
        @State
        private var hoverTask: DispatchWorkItem?

        var body: some View {
            SearchSourceToggles(
                musicBrainzSelected: $musicBrainzSelected,
                discogsSelected: $discogsSelected,
                discogsEnabled: false,
                showDiscogsInfo: $showPopover,
                hoverTask: $hoverTask
            )
            .popover(isPresented: $showPopover, arrowEdge: .bottom) {
                DiscogsKeyPopover(
                    isPresented: $showPopover,
                    hoverTask: $hoverTask,
                    onOpenSettings: {}
                )
            }
            .padding()
        }
    }
#endif
