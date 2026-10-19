import SwiftUI

/// Select All for a list whose table holds only the rows it has loaded,
/// offered by the list while it has focus in the primary window.
///
/// The list's table is SwiftUI's and takes `selectAll:` itself, selecting the
/// rows it holds; so while the list offers this, Edit ▸ Select All and
/// Command-A run it instead (`SelectAllTarget`).
@MainActor
final class ShownRowsSelectAll {
    /// Select every row the focused list shows, loaded or not; nil while no
    /// such list has focus.
    var offered: (@MainActor () -> Void)?
}

extension EnvironmentValues {
    /// The primary window's; a preview or a test hosting a list without one
    /// has none.
    @Entry
    var shownRowsSelectAll: ShownRowsSelectAll?
}
