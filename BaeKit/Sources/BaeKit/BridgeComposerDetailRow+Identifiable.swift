import Foundation

/// A composer page row's `id` is core's: the row's number on the page.
extension BridgeComposerDetailRow: Identifiable {}

/// A composer's page rows.
public typealias ComposerDetailRowList = PaginatedList<BridgeComposerDetailRow>
