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

    /// Whether a library whose system is Heart shows these stars as a
    /// heart: three stars or more.
    public var isHeart: Bool {
        number >= 3
    }

    /// The stars a track's Heart or Remove heart item asks for, from the
    /// stars the person gave it: five stars turn the heart on, and the
    /// person's rating goes when it is already on.
    public static func afterHeartToggle(from stars: BridgeStars?)
        -> BridgeStars?
    {
        stars?.isHeart == true ? nil : .five
    }
}
