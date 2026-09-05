import BaeKit
import SwiftUI

struct AppearanceSettingsTab: View {
    var body: some View {
        Form {
            AppearanceControls()
        }
        .formStyle(.grouped)
        .scrollContentBackground(.hidden)
        .windowBackground()
    }
}

#if DEBUG
    #Preview("Appearance") {
        AppearanceSettingsTab()
            .frame(width: 500, height: 300)
            .appAppearance()
    }
#endif
