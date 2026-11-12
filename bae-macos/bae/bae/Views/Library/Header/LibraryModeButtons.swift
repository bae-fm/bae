import BaeKit
import SwiftUI

/// One menu item per browser mode, the selected one checkmarked, for the
/// header heading's dropdown.
struct LibraryModeButtons: MacMenuPart {
    let selected: LibraryBrowserMode
    let select: (LibraryBrowserMode) -> Void

    var entries: [MacMenuEntry] {
        LibraryBrowserMode.allCases.map { mode in
            .item(
                MacMenuItem(
                    verbatim: mode.displayName,
                    checked: selected == mode
                ) { select(mode) }
            )
        }
    }
}
