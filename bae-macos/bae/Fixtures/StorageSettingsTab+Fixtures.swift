import BaeKit
import Foundation
import SwiftUI

@testable import bae

/// The Storage pane over `store`, as every board state draws it.
@MainActor
func storageSettingsPreview(_ store: DeviceStorageStore) -> some View {
    StorageSettingsTab()
        .environment(store)
        .environment(PreviewData.configStore())
        .environment(Downloads.stub())
        .environment(UiStore())
}
