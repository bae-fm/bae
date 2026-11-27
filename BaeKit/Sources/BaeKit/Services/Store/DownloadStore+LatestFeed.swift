extension DownloadStore {
    /// Start the streams for releases moving to This Mac and their current files.
    public nonisolated func feedMovesToThisDevice(
        from appHandle: AppHandle,
        bugReporter: any BridgeBugReporter
    ) -> [LatestFeed] {
        switch appHandle.feedMoveToThisDeviceTransfers() {
        case .streams(let releases, let files):
            return [
                moveToThisDeviceTransfers.fed(bugReporter: bugReporter) {
                    releases
                },
                moveFileShares.fed(bugReporter: bugReporter) { files },
            ]
        }
    }
}
