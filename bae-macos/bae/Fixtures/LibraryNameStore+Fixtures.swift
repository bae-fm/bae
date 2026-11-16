import BaeKit
import Foundation
import SwiftUI

@testable import bae

extension LibraryNameStore {
    /// A store whose name is `name` once a screen follows it, for
    /// shots.
    static func stub(name: String) -> LibraryNameStore {
        LibraryNameStore(
            subscribe: { PulledValues(FixedSubscription(value: name)) },
            current: { name },
            showError: { _ in }
        )
    }
}
