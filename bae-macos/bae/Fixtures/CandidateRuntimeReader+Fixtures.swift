import BaeKit
import Foundation
import SwiftUI

@testable import bae

extension View {
    /// What the reader above takes its values from in a shot: the
    /// runtimes `runtimes` holds, by key, and the importer the views
    /// under it act through. A shot that draws a candidate's pane or
    /// the release editor's search mounts it, so it provides them and
    /// renders the same chain the app does.
    func candidateReaderPreviewEnvironment(
        runtimes: [Data: BridgeCandidateRuntimeSnapshot] = [:]
    ) -> some View {
        self
            .environment(Importer.stub())
            .environment(\.heldCandidateRuntimes, runtimes)
    }
}
