import BaeKit
import SwiftUI

/// A person's removed and replaced devices, collapsed until opened. They no
/// longer have access, so they offer no actions.
struct FormerDevicesDisclosure: View {
    let devices: [BridgeFormerDevice]
    let count: UInt32

    @State
    private var isExpanded = false

    var body: some View {
        FormDisclosure(isExpanded: $isExpanded) {
            ForEach(devices, id: \.id) { device in
                HStack(spacing: ThemeSpace.related) {
                    Text(device.name)
                        .themeText(.body)
                        .foregroundStyle(.secondary)
                    Text(device.state.label)
                        .themeText(.detail)
                        .foregroundStyle(.secondary)
                    Spacer()
                }
            }
        } label: {
            // An Int makes the key "Removed devices (%lld)", as the string
            // catalog has it.
            Text("Removed devices (\(Int(count)))")
                .themeText(.detail)
                .foregroundStyle(.secondary)
        }
    }
}
