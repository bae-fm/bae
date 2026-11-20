import BaeKit
import OSLog
import SwiftUI
import UniformTypeIdentifiers

private let logger = Logger.bae("DropInfo")

extension DropInfo {
    /// Read each plain-text string the drop carries, in no order, and hand
    /// them to `receive` on the main queue once every item has loaded. bae's
    /// own drags carry only text, so an item that fails to load, or loads as
    /// something else, is a bug: the drop reports it to the crash reporter,
    /// logs it, hands `failed` the line that says something went wrong, and
    /// writes none of what it carried.
    func loadPlainText(
        failed: @escaping @MainActor (DisplayError) -> Void,
        then receive: @escaping @MainActor ([String]) -> Void
    ) {
        let providers = itemProviders(for: [UTType.plainText])
        let serialQueue = DispatchQueue(label: "bae.drop-plain-text")
        nonisolated(unsafe) var strings: [String] = []
        nonisolated(unsafe) var failures: [String] = []
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
                else {
                    let failure =
                        error.map { String(reflecting: $0) }
                        ?? "loaded as \(String(describing: item.map { type(of: $0) }))"
                    serialQueue.sync { failures.append(failure) }
                }
                group.leave()
            }
        }
        group.notify(queue: .main) {
            MainActor.assumeIsolated {
                guard failures.isEmpty else {
                    let detail =
                        "a dropped item did not load as text: \(failures.joined(separator: "; "))"
                    logger.error("\(detail)")
                    CoreBugReporter()
                        .report(operation: "load a drop's text", detail: detail)
                    failed(
                        DisplayError(
                            line: localizedCoreString(
                                "core.error.category.internal"
                            )
                        )
                    )
                    return
                }
                receive(strings)
            }
        }
    }
}
