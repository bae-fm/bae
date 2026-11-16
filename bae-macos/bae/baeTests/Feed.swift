/// One feed of values a probe answers a read with: a test emits, the read
/// takes them in order.
final class Feed<Value: Sendable>: @unchecked Sendable {
    private let stream: AsyncThrowingStream<Value, any Error>
    private let continuation: AsyncThrowingStream<Value, any Error>.Continuation
    private var iterator: AsyncThrowingStream<Value, any Error>.AsyncIterator

    init() {
        (stream, continuation) = AsyncThrowingStream.makeStream()
        iterator = stream.makeAsyncIterator()
    }

    func emit(_ value: Value) { continuation.yield(value) }

    func fail(_ error: any Error) { continuation.finish(throwing: error) }

    func next() async throws -> Value {
        guard let value = try await iterator.next() else {
            throw CancellationError()
        }
        return value
    }
}
