#if os(macOS)
    import AppKit
    import SwiftUI

    /// A control's radio group for assistive technology: one radio button for
    /// each choice, in the choice's frame, and the group itself, which reads
    /// `groupLabel` when it has one. It takes no clicks, so the control under
    /// it keeps them.
    ///
    /// `frames` holds where each choice is, in the overlaid view's own space;
    /// a choice not measured yet has no radio button to read.
    public struct MacRadioGroupAccessibility: NSViewRepresentable {
        let groupLabel: String?
        let labels: [String]
        let frames: [CGRect?]
        let selectedIndex: Int
        let isEnabled: Bool
        let select: (Int) -> Void

        public init(
            groupLabel: String? = nil,
            labels: [String],
            frames: [CGRect?],
            selectedIndex: Int,
            isEnabled: Bool = true,
            select: @escaping (Int) -> Void
        ) {
            self.groupLabel = groupLabel
            self.labels = labels
            self.frames = frames
            self.selectedIndex = selectedIndex
            self.isEnabled = isEnabled
            self.select = select
        }

        public func makeNSView(context: Context) -> RadioGroupView {
            RadioGroupView()
        }

        public func updateNSView(_ view: RadioGroupView, context: Context) {
            view.show(self)
        }

        public final class RadioGroupView: NSView {
            private var radios: [Radio] = []
            fileprivate var groupLabel: String?
            fileprivate var frames: [CGRect?] = []
            fileprivate var selectedIndex = 0
            fileprivate var isEnabled = true
            fileprivate var select: (Int) -> Void = { _ in }

            override public var isFlipped: Bool { true }

            override public func hitTest(_ point: NSPoint) -> NSView? { nil }

            fileprivate func show(_ spec: MacRadioGroupAccessibility) {
                if radios.map(\.label) != spec.labels {
                    radios = spec.labels.indices.map {
                        Radio(group: self, index: $0, label: spec.labels[$0])
                    }
                }
                groupLabel = spec.groupLabel
                frames = spec.frames
                selectedIndex = spec.selectedIndex
                isEnabled = spec.isEnabled
                select = spec.select
            }

            override public func isAccessibilityElement() -> Bool { true }

            override public func accessibilityRole() -> NSAccessibility.Role? {
                .radioGroup
            }

            override public func accessibilityLabel() -> String? {
                groupLabel
            }

            /// The group is what is off while no choice can be made; its
            /// radio buttons stay enabled, as the design draws them.
            override public func isAccessibilityEnabled() -> Bool {
                isEnabled
            }

            override public func accessibilityChildren() -> [Any]? {
                measuredRadios
            }

            override public func accessibilityContents() -> [Any]? {
                measuredRadios
            }

            private var measuredRadios: [Radio] {
                radios.indices.filter { frames[$0] != nil }.map { radios[$0] }
            }
        }

        /// One choice's radio button, reading the group for its place and
        /// state.
        private final class Radio: NSAccessibilityElement {
            private weak var group: RadioGroupView?
            private let index: Int
            let label: String

            init(group: RadioGroupView, index: Int, label: String) {
                self.group = group
                self.index = index
                self.label = label
                super.init()
            }

            override func accessibilityParent() -> Any? { group }

            override func accessibilityRole() -> NSAccessibility.Role? {
                .radioButton
            }

            override func accessibilityLabel() -> String? { label }

            /// Enabled whatever the group is: the design draws the radio
            /// buttons of an off control enabled, under a group that is off.
            override func isAccessibilityEnabled() -> Bool { true }

            override func isAccessibilitySelected() -> Bool {
                let group = group
                let index = index
                return MainActor.assumeIsolated {
                    group?.selectedIndex == index
                }
            }

            override func accessibilityValue() -> Any? {
                isAccessibilitySelected() ? 1 : 0
            }

            override func accessibilityFrame() -> NSRect {
                let group = group
                let index = index
                return MainActor.assumeIsolated {
                    guard let group, let window = group.window,
                        group.frames.indices.contains(index),
                        let frame = group.frames[index]
                    else { return .zero }
                    return window.convertToScreen(
                        group.convert(frame, to: nil)
                    )
                }
            }

            override func accessibilityPerformPress() -> Bool {
                let group = group
                let index = index
                return MainActor.assumeIsolated {
                    guard let group, group.isEnabled else { return false }
                    group.select(index)
                    return true
                }
            }
        }
    }
#endif
