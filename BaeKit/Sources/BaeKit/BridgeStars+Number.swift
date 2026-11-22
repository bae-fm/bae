extension BridgeStars: CaseIterable {
    /// The stars from one up. Swift generates `allCases` only when the
    /// `CaseIterable` conformance is in the file that declares the enum, and
    /// the generated bindings declare `BridgeStars` in another file.
    public static let allCases: [BridgeStars] = [
        .one, .two, .three, .four, .five,
    ]

    /// How many stars this is, one to five.
    public var number: Int {
        switch self {
        case .one: 1
        case .two: 2
        case .three: 3
        case .four: 4
        case .five: 5
        }
    }
}
