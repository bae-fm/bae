import BaeKit
import SwiftUI

/// Reads the stores for `MacFindOnline`'s Discogs bar: whether Discogs can be
/// asked, which updates while the pane is open, and whether the person put the
/// bar away for the session. The caller builds the pane from the two values.
struct MacFindOnlineContainer: View {
    @Environment(ConfigStore.self)
    private var configStore
    @Environment(UiStore.self)
    private var uiStore

    let pane:
        (_ showsDiscogsBar: Bool, _ onDismissDiscogsBar: @escaping () -> Void)
            -> MacFindOnline

    var body: some View {
        pane(
            MacFindOnline.showsDiscogsBar(
                configStore.config,
                dismissed: uiStore.discogsNoticeDismissed
            ),
            { uiStore.dismissDiscogsNotice() }
        )
    }
}
