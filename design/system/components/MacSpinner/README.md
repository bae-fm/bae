# MacSpinner

macOS's own spinner: eight spokes fading round the wheel. For a wait with no length and no words of its own, such as a list still filling. With `label`, the words say what is happening, ending in an ellipsis: "Looking for receivers…". A wait whose length is known, or a step of a flow, is a `MacProgress`.

**In the code:** macOS: BaeKit's `WaitingIndicator` (`Views/WaitingIndicator.swift`); `ProgressView()` is still used bare in `WelcomeLibraryDetails`, `LocalLibrariesSection`, `ActivityList`, `FindOnlineSection` and `FindOnlineSearchResults`.
