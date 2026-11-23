extension BridgeStars {
    /// The stars a tap on this star asks for when the person's rating is
    /// `mine`: the same stars again clears it.
    public func afterTapping(from mine: BridgeStars?) -> BridgeStars? {
        self == mine ? nil : self
    }
}
