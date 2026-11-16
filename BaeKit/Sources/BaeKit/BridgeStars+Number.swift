extension BridgeStars: CaseIterable {
    /// The stars from one up. Swift writes this list only for an enum it
    /// declares itself, and the bindings declare this one.
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
