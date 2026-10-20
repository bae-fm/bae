/// A lookup a screen shows as one section: still reading, read, or failed. A
/// failure is kept apart from an empty result because they show different
/// screens: a locked keychain doesn't mean there are no restore codes.
public enum SectionLoad<Value> {
    case loading
    case loaded(Value)
    case failed(DisplayError)

    public var value: Value? {
        if case .loaded(let value) = self { return value }
        return nil
    }

    public var failure: DisplayError? {
        if case .failed(let failure) = self { return failure }
        return nil
    }
}
