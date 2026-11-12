import BaeKit
import SwiftUI

struct AppearanceSettingsTab: View {
    var body: some View {
        SettingsPane(.appearance) {
            Form {
                Section {
                    AppearanceControls()
                }
            }
            .formStyle(.settings)
        }
    }
}

#if DEBUG
    #Preview("Appearance") {
        AppearanceSettingsTab()
            .frame(width: 500, height: 300)
            .windowBackground()
            .appAppearance()
    }
#endif
