import ObjectiveC
import SwiftUI

/// Disable NSHostingView's automatic min/max/intrinsic size computation.
/// Without this, every @Observable property change triggers full view-tree
/// sizing walks (~7% CPU during playback). The window keeps its SwiftUI
/// minWidth/minHeight constraints; we just stop the hosting view from
/// redundantly recomputing them every display cycle.
@MainActor
private func disableHostingViewSizingOptions(_ window: NSWindow) {
    guard let contentView = window.contentView else {
        return
    }
    let sel = NSSelectorFromString("setSizingOptions:")
    guard contentView.responds(to: sel) else {
        return
    }
    guard let imp = class_getMethodImplementation(type(of: contentView), sel)
    else { return }
    typealias Fn = @convention(c) (AnyObject, Selector, Int) -> Void
    unsafeBitCast(imp, to: Fn.self)(contentView, sel, 0)
}

/// Adjusts the position of the window's traffic light buttons (close/minimize/zoom).
/// With .hiddenTitleBar, the buttons sit at the system default position which doesn't align
/// with our custom title bar content. This modifier accesses the NSWindow and shifts them.
/// Reapplies on window resize since macOS resets button positions during resize.
struct TrafficLightOffset: ViewModifier {
    let xOffset: CGFloat
    let yOffset: CGFloat

    func body(content: Content) -> some View {
        content
            .background(TrafficLightHelper(xOffset: xOffset, yOffset: yOffset))
    }

    private struct TrafficLightHelper: NSViewRepresentable {
        let xOffset: CGFloat
        let yOffset: CGFloat

        func makeNSView(context: Context) -> NSView {
            let view = NSView()
            DispatchQueue.main.async {
                guard let window = view.window else {
                    return
                }
                adjustButtons(in: window)
                context.coordinator.observeResize(
                    window: window,
                    xOffset: xOffset,
                    yOffset: yOffset
                )
                disableHostingViewSizingOptions(window)
            }
            return view
        }

        func updateNSView(_: NSView, context _: Context) {}

        func makeCoordinator() -> Coordinator {
            Coordinator()
        }

        private func adjustButtons(in window: NSWindow) {
            for buttonType: NSWindow.ButtonType in [
                .closeButton, .miniaturizeButton, .zoomButton,
            ] {
                guard let button = window.standardWindowButton(buttonType)
                else {
                    continue
                }
                var origin = button.frame.origin
                origin.x += xOffset
                origin.y -= yOffset
                button.setFrameOrigin(origin)
            }
        }

        class Coordinator: NSObject {
            private var observation: Any?

            func observeResize(
                window: NSWindow,
                xOffset: CGFloat,
                yOffset: CGFloat
            ) {
                observation = NotificationCenter.default.addObserver(
                    forName: NSWindow.didResizeNotification,
                    object: window,
                    queue: .main,
                ) { notification in
                    guard let window = notification.object as? NSWindow else {
                        return
                    }
                    MainActor.assumeIsolated {
                        for buttonType: NSWindow.ButtonType in [
                            .closeButton, .miniaturizeButton, .zoomButton,
                        ] {
                            guard
                                let button = window.standardWindowButton(
                                    buttonType
                                )
                            else {
                                continue
                            }
                            var origin = button.frame.origin
                            origin.x += xOffset
                            origin.y -= yOffset
                            button.setFrameOrigin(origin)
                        }
                    }
                }
            }

            deinit {
                if let observation {
                    NotificationCenter.default.removeObserver(observation)
                }
            }
        }
    }
}
