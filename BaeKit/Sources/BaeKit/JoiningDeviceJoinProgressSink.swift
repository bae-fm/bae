import Foundation

private final class LatestProgressDelivery<Value: Sendable>: @unchecked Sendable
{
    private let continuation: AsyncStream<Value>.Continuation
    private let task: Task<Void, Never>

    init(apply: @escaping @MainActor @Sendable (Value) -> Void) {
        let stream = AsyncStream.makeStream(
            of: Value.self,
            bufferingPolicy: .bufferingNewest(1)
        )
        continuation = stream.continuation
        task = Task { @MainActor in
            for await value in stream.stream {
                apply(value)
            }
        }
    }

    func yield(_ value: Value) {
        continuation.yield(value)
    }

    deinit {
        continuation.finish()
        task.cancel()
    }
}

public final class JoiningDeviceJoinProgressSink:
    JoiningDeviceJoinProgressCallback, @unchecked Sendable
{
    private let delivery:
        LatestProgressDelivery<
            BridgeJoiningDeviceJoinProgress
        >

    public init(
        apply:
            @escaping @MainActor @Sendable (
                BridgeJoiningDeviceJoinProgress
            ) -> Void
    ) {
        delivery = LatestProgressDelivery(apply: apply)
    }

    public func onProgress(progress: BridgeJoiningDeviceJoinProgress) {
        delivery.yield(progress)
    }
}

public final class AdmittingDeviceJoinProgressSink:
    AdmittingDeviceJoinProgressCallback, @unchecked Sendable
{
    private let delivery:
        LatestProgressDelivery<
            BridgeAdmittingDeviceJoinProgress
        >

    public init(
        apply:
            @escaping @MainActor @Sendable (
                BridgeAdmittingDeviceJoinProgress
            ) -> Void
    ) {
        delivery = LatestProgressDelivery(apply: apply)
    }

    public func onProgress(progress: BridgeAdmittingDeviceJoinProgress) {
        delivery.yield(progress)
    }
}
