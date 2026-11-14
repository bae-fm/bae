extension BridgeStars {
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

    /// The stars of this `number`, `nil` outside one to five.
    public init?(number: Int) {
        switch number {
        case 1: self = .one
        case 2: self = .two
        case 3: self = .three
        case 4: self = .four
        case 5: self = .five
        default: return nil
        }
    }
}
