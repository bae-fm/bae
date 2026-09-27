import BaeKit
import SwiftUI

/// The eyebrow above a welcome-screen section, optionally trailed by an info
/// tip.
struct WelcomeSectionHeader: View {
    let title: LocalizedStringKey
    var infoTip: InfoTip?

    var body: some View {
        HStack(spacing: 4) {
            Eyebrow(title)
            infoTip
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

#if DEBUG
    #Preview("Plain") {
        WelcomeSectionHeader(title: "On this device")
            .frame(width: 400)
            .padding()
    }

    #Preview("With info tip") {
        WelcomeSectionHeader(
            title: "Restore from iCloud Keychain",
            infoTip: InfoTip(
                text:
                    "Libraries whose restore codes are saved in your keychain."
            ),
        )
        .frame(width: 400)
        .padding()
    }
#endif
