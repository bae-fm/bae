import AppKit
import BaeKit
import SwiftUI

/// NSSegmentedControl wrapper that supports disabling individual segments.
struct SourceSegmentedControl: NSViewRepresentable {
    @Binding
    var selection: BridgeMetadataSource
    let discogsEnabled: Bool
    @Binding
    var showDiscogsInfo: Bool
    @Binding
    var hoverTask: DispatchWorkItem?

    func makeNSView(context: Context) -> TrackingSegmentedControl {
        let control = TrackingSegmentedControl(
            labels: ["MusicBrainz", "Discogs"],
            trackingMode: .selectOne,
            target: context.coordinator,
            action: #selector(Coordinator.segmentChanged(_:))
        )
        control.segmentStyle = .rounded
        control.selectedSegment = selection == .musicBrainz ? 0 : 1
        control.setEnabled(discogsEnabled, forSegment: 1)
        control.setWidth(100, forSegment: 0)
        control.setWidth(100, forSegment: 1)
        control.coordinator = context.coordinator
        return control
    }

    func updateNSView(_ control: TrackingSegmentedControl, context: Context) {
        control.selectedSegment = selection == .musicBrainz ? 0 : 1
        control.setEnabled(discogsEnabled, forSegment: 1)
        context.coordinator.parent = self
    }

    func makeCoordinator() -> Coordinator {
        Coordinator(parent: self)
    }

    class TrackingSegmentedControl: NSSegmentedControl {
        weak var coordinator: Coordinator?

        override func updateTrackingAreas() {
            super.updateTrackingAreas()
            for area in trackingAreas {
                removeTrackingArea(area)
            }
            addTrackingArea(
                NSTrackingArea(
                    rect: bounds,
                    options: [
                        .mouseEnteredAndExited, .mouseMoved, .activeInActiveApp,
                    ],
                    owner: self,
                    userInfo: nil,
                )
            )
        }

        override func mouseEntered(with event: NSEvent) {
            coordinator?.handleMouse(event, entered: true)
        }

        override func mouseMoved(with event: NSEvent) {
            coordinator?.handleMouseMoved(event, in: self)
        }

        override func mouseExited(with event: NSEvent) {
            coordinator?.handleMouse(event, entered: false)
        }
    }

    @MainActor
    class Coordinator: NSObject {
        var parent: SourceSegmentedControl
        private var isOverDiscogs = false

        init(parent: SourceSegmentedControl) {
            self.parent = parent
        }

        @objc
        func segmentChanged(_ sender: NSSegmentedControl) {
            parent.selection =
                sender.selectedSegment == 0 ? .musicBrainz : .discogs
        }

        func handleMouse(_: NSEvent, entered: Bool) {
            guard !parent.discogsEnabled else {
                return
            }
            if !entered, isOverDiscogs {
                isOverDiscogs = false
                showPopoverDelayed(false)
            }
        }

        func handleMouseMoved(_ event: NSEvent, in control: NSSegmentedControl)
        {
            guard !parent.discogsEnabled else {
                return
            }
            let location = control.convert(event.locationInWindow, from: nil)
            let midX = control.bounds.width / 2
            let overDiscogs = location.x > midX

            if overDiscogs, !isOverDiscogs {
                isOverDiscogs = true
                showPopoverDelayed(true)
            }
            else if !overDiscogs, isOverDiscogs {
                isOverDiscogs = false
                showPopoverDelayed(false)
            }
        }

        private func showPopoverDelayed(_ show: Bool) {
            parent.hoverTask?.cancel()
            let task = DispatchWorkItem { [weak self] in
                self?.parent.showDiscogsInfo = show
            }
            parent.hoverTask = task
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.3, execute: task)
        }
    }
}

#if DEBUG
    #Preview("Source Picker - Discogs Disabled") {
        SourcePickerPreview(hasKey: false)
            .frame(width: 500, height: 100)
            .windowBackground()
    }

    private struct SourcePickerPreview: View {
        let hasKey: Bool
        @State
        private var source: BridgeMetadataSource = .musicBrainz
        @State
        private var showPopover = false
        @State
        private var hoverTask: DispatchWorkItem?

        var body: some View {
            SourceSegmentedControl(
                selection: $source,
                discogsEnabled: hasKey,
                showDiscogsInfo: $showPopover,
                hoverTask: $hoverTask,
            )
            .frame(width: 200)
            .overlay(alignment: .bottomTrailing) {
                Color.clear
                    .frame(width: 100, height: 1)
                    .popover(isPresented: $showPopover, arrowEdge: .bottom) {
                        DiscogsKeyPopover(
                            isPresented: $showPopover,
                            hoverTask: $hoverTask,
                            onOpenSettings: {},
                        )
                    }
                    .allowsHitTesting(false)
            }
            .padding()
        }
    }
#endif
