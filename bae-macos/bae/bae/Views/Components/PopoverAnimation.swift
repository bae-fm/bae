import AppKit

/// No popover in bae animates.
///
/// A popover's pop in/out animation runs its frames on the main thread
/// alongside the content's first SwiftUI build, and any non-trivial content
/// (the queue's row window, the signal chips' detail tables) starves them into
/// a visible stutter both directions. An instant show/dismiss reads like a
/// menu instead — menus feel crisp precisely because they don't animate.
/// `PopoverEntrance` springs the content in instead.
///
/// `animates = false` also covers content-size updates. That is load-bearing:
/// AppKit animates a presented popover's resize by spinning a nested run loop
/// (`NSMoveHelper _doAnimation`), which can call out to a torn-down run-loop
/// observer and segfault.
///
/// SwiftUI's `.popover` hands its content no handle on the `NSPopover` it
/// presents, so the policy is set where every popover announces itself: the
/// will-show notification, whose object is the popover about to show.
enum PopoverAnimation {
    /// Turn off the animation of every popover this process shows from now
    /// on, as each is about to show. The observation lasts as long as the
    /// process does, so it is never removed.
    @MainActor
    static func disableForEveryPopover() {
        _ = NotificationCenter.default.addObserver(
            forName: NSPopover.willShowNotification,
            object: nil,
            queue: nil
        ) { notification in
            guard let popover = notification.object as? NSPopover else {
                preconditionFailure(
                    "A popover's will-show notification carried no popover"
                )
            }
            MainActor.assumeIsolated {
                popover.animates = false
            }
        }
    }
}
