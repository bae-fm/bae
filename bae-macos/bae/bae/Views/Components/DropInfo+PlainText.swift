import BaeKit
import OSLog
import SwiftUI
import UniformTypeIdentifiers

private let logger = Logger.bae("DropInfo")

extension DropInfo {
    /// Read each plain-text string the drop carries, in no order, and hand
    /// them to `receive` on the main queue once every item has loaded. An
    /// item that fails to load, or loads as something other than text, is
    /// logged and left out.
    func loadPlainText(then receive: @escaping @MainActor ([String]) -> Void) {
        let providers = itemProviders(for: [UTType.plainText])
        let serialQueue = DispatchQueue(label: "bae.drop-plain-text")
        nonisolated(unsafe) var strings: [String] = []
        let group = DispatchGroup()
        for provider in providers {
            group.enter()
            provider.loadItem(forTypeIdentifier: UTType.plainText.identifier) {
                item,
                error in
                if let data = item as? Data,
                    let string = String(data: data, encoding: .utf8)
                {
                    serialQueue.sync { strings.append(string) }
                }
                else if let string = item as? String {
                    serialQueue.sync { strings.append(string) }
                }
                else if let error {
                    logger.warning(
                        "a dropped item failed to load as text: \(error.localizedDescription)"
                    )
                }
                else {
                    logger.warning(
                        "a dropped item loaded as \(String(describing: item.map { type(of: $0) })), not text"
                    )
                }
                group.leave()
            }
        }
        group.notify(queue: .main) {
            MainActor.assumeIsolated { receive(strings) }
        }
    }
}
