/// A tick for the import-audition seek bar: either a position update (progress
/// fraction plus the elapsed milliseconds) or a reset back to the start.
enum PreviewProgressEvent {
    case position(progress: Double, positionMs: UInt64)
    case reset
}
