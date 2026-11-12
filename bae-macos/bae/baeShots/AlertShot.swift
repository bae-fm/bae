import BaeKit
import SwiftUI

/// An alert as the app presents it, drawn in place instead of in a sheet so
/// a shot can capture it: bae's alert face centred in the scene.
struct AlertShot: View {
    let alert: (MacAlertWords, [MacAlertButton])

    var body: some View {
        MacAlertView(alert.0, buttons: alert.1)
            .elevatedFace()
            .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}
