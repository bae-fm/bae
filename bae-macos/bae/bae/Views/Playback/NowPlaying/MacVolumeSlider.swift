import AppKit
import BaeKit
import OSLog
import SwiftUI

/// The volume at the end of the now playing bar: mute and a knobless slider,
/// or, while playback is on a receiver that takes no volume from bae, the line
/// that says where the volume is set. While core has not said the settings,
/// the controls keep their place and show nothing.
struct MacVolumeSlider: View {
    /// What the volume offers.
    enum Controls: Equatable {
        /// The receiver playback is on keeps its own volume: a line names it
        /// in place of mute and the slider, which could only be refused.
        case setOnReceiver(String)
        case controls(PlaybackSettings)
        /// Core has not said the settings yet: the controls keep their place.
        case waitingForCore
    }

    let controls: Controls
    /// The level a drag is at: moves the output, stores nothing.
    let onDrag: (Float) -> Void
    /// The level a drag or click ends on.
    let onChange: (Float) async -> Void
    /// Mute or unmute: the value the mute button sets.
    let onSetMuted: (Bool) -> Void

    private static let sliderWidth: CGFloat = 96

    var body: some View {
        HStack(spacing: ThemeSpace.group) {
            switch controls {
            case .setOnReceiver(let name):
                Text("Volume is set on \(name)")
                    .themeText(.detail)
                    .foregroundStyle(Theme.macTextSecondary)
                    .lineLimit(1)
            case .controls(let settings):
                muteButton(settings)

                VolumeSliderTrack(
                    value: settings.volume,
                    onDrag: onDrag,
                    onChange: onChange
                )
                .frame(width: Self.sliderWidth)
            case .waitingForCore:
                MacTransport.controlSpace

                Color.clear.frame(width: Self.sliderWidth, height: 1)
            }
        }
    }

    private func muteButton(_ settings: PlaybackSettings) -> some View {
        let isMuted = settings.isMuted
        return MacIconButton(
            symbol: Self.muteIconName(settings),
            label: isMuted ? "Unmute" : "Mute"
        ) {
            onSetMuted(!isMuted)
        }
    }

    /// The speaker glyph for the volume: silenced when muted or at zero, one
    /// wave up to 0.55, two above it.
    private static func muteIconName(_ settings: PlaybackSettings) -> String {
        if settings.isMuted || settings.volume == 0 {
            return "speaker.slash.fill"
        }
        return settings.volume <= 0.55
            ? "speaker.wave.1.fill" : "speaker.wave.2.fill"
    }
}

private let volumeSliderLogger = Logger.bae("MacVolumeSlider")

/// A knobless volume slider. During a click or drag it shows the level under
/// the pointer and reports each one through `onDrag`. The slider sets the
/// level the gesture ends on once through `onChange` and shows that level
/// until the call returns, and then shows `value`.
private struct VolumeSliderTrack: View {
    let value: Float
    let onDrag: (Float) -> Void
    let onChange: (Float) async -> Void

    /// The level under the pointer, from the gesture's start until the call that sets it returns.
    @State
    private var dragValue: Float?

    /// The track's thickness.
    private static let trackHeight: CGFloat = 5

    /// The height of the area that takes a click or drag.
    private static let targetHeight: CGFloat = 20

    private static func level(at x: CGFloat, width: CGFloat) -> Float {
        Float(max(0, min(1, x / width)))
    }

    private var clamped: Float { max(0, min(1, dragValue ?? value)) }

    /// Show `level` and set it through `onChange`, until that set returns.
    private func set(_ level: Float) {
        dragValue = level
        Task {
            await onChange(level)
            // When a new drag started while this call ran, keep that drag's level.
            if dragValue == level {
                dragValue = nil
            }
        }
    }

    var body: some View {
        GeometryReader { geo in
            let width = geo.size.width
            ZStack(alignment: .leading) {
                Capsule()
                    .fill(Theme.hairline)
                Capsule()
                    .fill(Theme.accent)
                    .frame(width: CGFloat(clamped) * width)
            }
            .frame(width: width, height: Self.trackHeight)
            .accessibilityHidden(true)
            // The pointer's target is taller than the track and takes no
            // layout room: the slider is as tall as its track.
            .contentShape(
                Rectangle()
                    .inset(
                        by: -(Self.targetHeight - Self.trackHeight) / 2
                    )
            )
            .gesture(
                DragGesture(minimumDistance: 0)
                    .onChanged { drag in
                        guard width > 0 else { return }
                        let level = Self.level(
                            at: drag.location.x,
                            width: width
                        )
                        dragValue = level
                        onDrag(level)
                    }
                    .onEnded { drag in
                        guard width > 0 else {
                            dragValue = nil
                            return
                        }
                        let level = Self.level(
                            at: drag.location.x,
                            width: width
                        )
                        set(level)
                    },
            )
        }
        .frame(height: Self.trackHeight)
        .background {
            VolumeAccessibilityElement(
                percent: AccessibilityPercentage.percent(for: Double(clamped)),
                set: { set(Float(AccessibilityPercentage.fraction(for: $0))) }
            )
        }
    }
}

/// The slider as assistive clients see it: a slider the height of the track,
/// which says its level as a whole percent and steps by 5. SwiftUI's own
/// `Slider` gives an element at least 20 pt tall.
private struct VolumeAccessibilityElement: NSViewRepresentable {
    let percent: Double
    let set: (Double) -> Void

    func makeNSView(context _: Context) -> View {
        View()
    }

    func updateNSView(_ view: View, context _: Context) {
        view.percent = percent
        view.set = set
    }

    final class View: NSView {
        var percent: Double = 0
        var set: (Double) -> Void = { _ in }

        private static let step = 5.0

        override func isAccessibilityElement() -> Bool { true }
        override func accessibilityRole() -> NSAccessibility.Role? { .slider }
        override func accessibilityLabel() -> String? {
            String(localized: "Volume")
        }
        override func accessibilityValue() -> Any? { Int(percent) }
        override func accessibilityMinValue() -> Any? {
            AccessibilityPercentage.range.lowerBound
        }
        override func accessibilityMaxValue() -> Any? {
            AccessibilityPercentage.range.upperBound
        }
        override func setAccessibilityValue(_ value: Any?) {
            guard let value = (value as? NSNumber)?.doubleValue else {
                volumeSliderLogger.error(
                    "Ignored a volume that is no number: \(String(describing: value))"
                )
                return
            }
            set(value)
        }
        override func accessibilityPerformIncrement() -> Bool {
            set(percent + Self.step)
            return true
        }
        override func accessibilityPerformDecrement() -> Bool {
            set(percent - Self.step)
            return true
        }
        override func hitTest(_: NSPoint) -> NSView? { nil }
    }
}
