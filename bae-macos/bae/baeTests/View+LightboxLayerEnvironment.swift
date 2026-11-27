import BaeKit
import SwiftUI

@testable import bae

extension View {
    /// This view with what its lightbox, and the layers it presents, read
    /// from the environment: `images` and a bug reporter.
    func lightboxLayerEnvironment(_ images: ImageStore) -> some View {
        layerEnvironment {
            $0.environment(images)
                .environment(BugReports.recording())
        }
    }
}
