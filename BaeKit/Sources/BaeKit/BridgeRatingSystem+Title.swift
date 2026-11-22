import Foundation

extension BridgeRatingSystem: CaseIterable {
    /// The systems in the order a pop-up lists them. Swift generates
    /// `allCases` only when the `CaseIterable` conformance is in the file
    /// that declares the enum, and the generated bindings declare
    /// `BridgeRatingSystem` in another file.
    public static let allCases: [BridgeRatingSystem] = [
        .stars, .heart, .hidden,
    ]

    /// The name Library settings gives the system.
    public var title: String {
        switch self {
        case .stars: String(localized: "Stars")
        case .heart: String(localized: "Heart")
        case .hidden: String(localized: "None")
        }
    }
}
