import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacCopyText` in a Subsonic address row.
    static let macCopyTextScenes: [ShotScene] = [
        copyText(
            "mac.component.copy-text.one-address",
            [("http://192.168.1.20:4533", nil)],
            hover: false
        ),
        copyText(
            "mac.component.copy-text.hovered",
            [("http://192.168.1.20:4533", nil)],
            hover: true
        ),
        copyText(
            "mac.component.copy-text.several-addresses",
            [
                ("http://192.168.1.20:4533", "(Wi-Fi)"),
                ("http://100.64.0.3:4533", "(Tailscale)"),
            ],
            hover: false
        ),
    ]

    private static func copyText(
        _ id: String,
        _ addresses: [(String, String?)],
        hover: Bool
    ) -> ShotScene {
        component(id, height: 130) {
            ShotBox {
                VStack(alignment: .leading, spacing: ThemeSpace.line) {
                    Text("Available on")
                    VStack(alignment: .leading, spacing: ThemeSpace.line) {
                        if addresses.count == 1 {
                            HStack(spacing: ThemeSpace.inline) {
                                Text("Apps connect at")
                                MacCopyText(
                                    text: addresses[0].0,
                                    after: addresses[0].1,
                                    hover: hover,
                                    copy: { _ in }
                                )
                            }
                        }
                        else {
                            Text("Apps connect at:")
                            ForEach(addresses, id: \.0) { address in
                                HStack(
                                    alignment: .firstTextBaseline,
                                    spacing: ThemeSpace.inline
                                ) {
                                    Text(verbatim: "•")
                                    MacCopyText(
                                        text: address.0,
                                        after: address.1,
                                        hover: hover,
                                        copy: { _ in }
                                    )
                                }
                            }
                        }
                    }
                    .themeText(.detail)
                    .foregroundStyle(.secondary)
                }
            }
        }
    }
}
