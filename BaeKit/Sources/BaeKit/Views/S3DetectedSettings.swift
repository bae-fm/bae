import SwiftUI

/// What S3 detection found: the bucket's versioning, and the settings and
/// probe results as coven recorded them, for someone setting up an unknown
/// service.
public struct S3DetectedSettings: View {
    private let detection: BridgeS3Detection

    public init(detection: BridgeS3Detection) {
        self.detection = detection
    }

    public var body: some View {
        switch detection.versioning() {
        case .known(let state):
            LabeledContent("Bucket versioning", value: state.label)
        case .failed(_, let detail):
            ErrorDetailDisclosure(
                error: DisplayError(
                    line: String(localized: "Couldn't read bucket versioning."),
                    detail: detail
                )
            )
        }
        DisclosureGroup("Detected settings") {
            Text(verbatim: String(describing: detection.settings()))
                .themeText(.mono)
                .textSelection(.enabled)
            ForEach(
                Array(detection.results().enumerated()),
                id: \.offset
            ) { _, result in
                Label {
                    Text(verbatim: "\(result.probe): \(result.detail)")
                        .themeText(.mono)
                } icon: {
                    Image(
                        systemName: result.held
                            ? "checkmark.circle" : "xmark.circle"
                    )
                }
            }
        }
    }
}
