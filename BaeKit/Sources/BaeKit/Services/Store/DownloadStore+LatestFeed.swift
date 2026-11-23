extension DownloadStore {
    /// Start the streams for releases moving to This Mac and their current files.
    public nonisolated func feedMovesToThisDevice(from appHandle: AppHandle)
        -> [LatestFeed]
    {
        switch appHandle.feedMoveToThisDeviceTransfers() {
        case .streams(let releases, let files):
            return [
                moveToThisDeviceTransfers.fed { releases },
                moveFileShares.fed { files },
            ]
        }
    }
}
