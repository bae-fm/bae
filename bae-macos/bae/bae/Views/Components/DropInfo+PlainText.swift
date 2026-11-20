import SwiftUI
import UniformTypeIdentifiers

extension DropInfo {
    /// Read each plain-text string the drop carries, in no order, and hand
    /// them to `receive` on the main queue once every item has loaded.
    func loadPlainText(then receive: @escaping @MainActor ([String]) -> Void) {
        let providers = itemProviders(for: [UTType.plainText])
        let serialQueue = DispatchQueue(label: "bae.drop-plain-text")
        nonisolated(unsafe) var strings: [String] = []
        let group = DispatchGroup()
        for provider in providers {
            group.enter()
            provider.loadItem(forTypeIdentifier: UTType.plainText.identifier) {
                item,
                _ in
                if let data = item as? Data,
                    let string = String(data: data, encoding: .utf8)
                {
                    serialQueue.sync { strings.append(string) }
                }
                else if let string = item as? String {
                    serialQueue.sync { strings.append(string) }
                }
                group.leave()
            }
        }
        group.notify(queue: .main) {
            MainActor.assumeIsolated { receive(strings) }
        }
    }
}
