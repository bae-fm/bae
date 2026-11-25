import AppKit

/// What the window server and the displays held when a scene's window was
/// not shown: the failure names it, so whoever reads the failure sees why
/// the window was missing.
struct ShotWindowState: Sendable, Equatable {
    /// The window server's entry for one window, which it lists whether or
    /// not the window is on the screen.
    struct Entry: Sendable, Equatable {
        let id: CGWindowID
        let frame: CGRect
        let layer: Int
        let onScreen: Bool
    }

    /// One display AppKit lists.
    struct Display: Sendable, Equatable {
        let id: CGDirectDisplayID
        let frame: CGRect
        let scale: CGFloat
    }

    /// The window server's entry for one of the scene's windows, or nil when
    /// it lists none.
    struct Listing: Sendable, Equatable {
        let id: CGWindowID
        let entry: Entry?
    }

    /// One listing for each of the scene's windows, in order.
    let windows: [Listing]
    let displays: [Display]
    /// The display the scene is drawn on.
    let shotDisplay: CGDirectDisplayID
    /// Who holds that display (`ShotDisplay.holder`).
    let holder: String

    /// The state now, for the windows numbered `windowIDs`.
    @MainActor
    static func read(
        windowIDs: [CGWindowID],
        on display: ShotDisplay
    ) -> Self {
        let listed =
            (CGWindowListCopyWindowInfo([.optionAll], kCGNullWindowID)
                as? [[String: Any]]) ?? []
        let entries = listed.compactMap(entry(from:))
        return Self(
            windows: windowIDs.map { id in
                Listing(id: id, entry: entries.first { $0.id == id })
            },
            displays: NSScreen.screens.compactMap { screen in
                screen.displayNumber.map {
                    Display(
                        id: $0,
                        frame: screen.frame,
                        scale: screen.backingScaleFactor
                    )
                }
            },
            shotDisplay: display.id,
            holder: display.holder
        )
    }

    private static func entry(from info: [String: Any]) -> Entry? {
        guard
            let number = info[kCGWindowNumber as String] as? Int,
            let bounds = info[kCGWindowBounds as String] as? [String: Any],
            let frame = CGRect(dictionaryRepresentation: bounds as CFDictionary)
        else { return nil }
        return Entry(
            id: CGWindowID(number),
            frame: frame,
            layer: info[kCGWindowLayer as String] as? Int ?? 0,
            // The window server leaves the key out for a window that is not
            // on the screen.
            onScreen: info[kCGWindowIsOnscreen as String] as? Bool ?? false
        )
    }

    /// The state as the failure prints it.
    var description: String {
        let windowLines = windows.map { listing in
            listing.entry.map {
                "window \(listing.id): window server frame \($0.frame), "
                    + "layer \($0.layer), "
                    + "\($0.onScreen ? "on" : "not on") the screen"
            } ?? "window \(listing.id): the window server lists no entry"
        }
        let displayLines = displays.map {
            "display \($0.id)\($0.id == shotDisplay ? " (the shot display)" : ""): "
                + "frame \($0.frame), \($0.scale)x"
        }
        return
            (windowLines + displayLines + ["shot display held as: \(holder)"])
            .joined(separator: "; ")
    }
}
