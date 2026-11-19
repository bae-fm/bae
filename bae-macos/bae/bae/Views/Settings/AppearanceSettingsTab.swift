import BaeKit
import SwiftUI

struct AppearanceSettingsTab: View {
    var body: some View {
        SettingsPane(.appearance) {
            Form {
                MacSection {
                    AppearanceControls()
                }
            }
            .formStyle(.settings)
        }
    }
}
