import BaeKit
import SwiftUI

/// A person's removed and replaced devices, collapsed until the person opens
/// them. These devices no longer have access, so the view shows no actions
/// for them.
struct FormerDevicesDisclosure: View {
    let devices: [BridgeFormerDevice]
    let count: UInt32

    @State
    private var isExpanded = false

    var body: some View {
        MacDisclosure(isExpanded: $isExpanded) {
            ForEach(devices, id: \.id) { device in
                HStack(spacing: ThemeSpace.related) {
                    Text(device.name)
                        .themeText(.body)
                        .foregroundStyle(Theme.textSecondary)
                    Text(device.state.label)
                        .themeText(.detail)
                        .foregroundStyle(Theme.textSecondary)
                    Spacer()
                }
            }
        } label: {
            // An Int makes the key "Removed devices (%lld)", as the string
            // catalog has it.
            Text("Removed devices (\(Int(count)))")
        }
    }
}
