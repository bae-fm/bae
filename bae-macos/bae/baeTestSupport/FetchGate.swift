/// Holds every fetch that reaches it until `open`, so a test decides when a load
/// in flight may finish.
actor FetchGate {
    private var isOpen = false
    private var held: [CheckedContinuation<Void, Never>] = []

    func pass() async {
        if isOpen {
            return
        }
        await withCheckedContinuation { held.append($0) }
    }

    func open() {
        isOpen = true
        for fetch in held {
            fetch.resume()
        }
        held = []
    }
}
