# MacWindow

A window as AppKit draws one whose content reaches under its title bar, with no chrome of the app's: the rounded window, the traffic lights 9 pt from its top and left where AppKit places them, and the content. It is as wide as its place unless `width` is given, and 240 pt tall unless `height` is given. A `MacSheet` with `overlay` inside is attached to it, as `MacSheet`'s preview shows one.

**In the code:** macOS: AppKit's `NSWindow`, which the shots draw a component's sheet in; the app has no view of its own for it.
