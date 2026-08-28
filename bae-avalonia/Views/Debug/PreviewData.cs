#if DEBUG
using System;
using System.Collections.Generic;
using System.Linq;
using uniffi.bae_bridge;

namespace Bae.Desktop;

// Static fixtures for the shot-capture scenes — the cross-platform analogue of
// the macOS PreviewData set. Generic placeholder values only: no real
// artist/album/song names, no bridge, keychain, or library access. Compiled only
// in DEBUG builds, alongside the capture harness it feeds.
internal static class PreviewData
{
    internal const string ImportRoot = "/Music/Incoming";

    // Fixture libraries for the welcome chooser: two on-disk libraries the welcome
    // scene lists as re-openable, plus the create/join/restore actions. Local only
    // (no cloud provider), inactive, no open error — placeholder ids/names, no
    // keychain or on-disk library behind them.
    internal static List<BridgeLibrary> WelcomeLibraries { get; } = new()
    {
        new BridgeLibrary("lib-home", "Home Library", @"C:\Users\Example\Music\Home", null, false, null),
        new BridgeLibrary("lib-studio", "Studio Library", @"C:\Users\Example\Music\Studio", null, false, null),
    };

    internal static List<BridgeWatchedFolder> ImportWatchedFolders { get; } =
        new() { new BridgeWatchedFolder(ImportRoot, "Incoming") };

    internal static BridgeFolderReleaseDecisionKey ImportGroupKey { get; } =
        new(ImportRoot, "Collection");

    // One window of the import list: the group header core emits before the run
    // of rows it holds, two grouped rows, an ungrouped row, and a boundary
    // still waiting on a decision.
    internal static List<BridgeImportListItem> ImportItems { get; } = new()
    {
        new BridgeImportListItem.GroupHeader(
            GroupStableKey(ImportGroupKey),
            new BridgeTriageGroup(ImportGroupKey, "Collection", Combinable: true),
            ImportRoot,
            true,
            2),
        ImportCandidateItem(
            "Release 01",
            "Collection/Release 01",
            isGroupMember: true),
        ImportCandidateItem(
            "Release 02",
            "Collection/Release 02",
            isGroupMember: true),
        ImportCandidateItem("Release 03", "Release 03", isGroupMember: false),
    };

    internal static BridgeImportQueueSummary ImportSummary { get; } = Summary(
        pending: 4,
        ready: new[]
        {
            ReadyRow("Collection/Release 01"),
            ReadyRow("Collection/Release 02"),
            ReadyRow("Release 03"),
        },
        groupKeys: new[] { ImportGroupKey });

    // The same queue with its one group folded shut: core emits the header and
    // none of the rows it holds.
    internal static List<BridgeImportListItem> ImportCollapsedItems { get; } = new List<BridgeImportListItem>
    {
        new BridgeImportListItem.GroupHeader(
            GroupStableKey(ImportGroupKey),
            new BridgeTriageGroup(ImportGroupKey, "Collection", Combinable: true),
            ImportRoot,
            false,
            2),
    }.Concat(ImportItems.Skip(3)).ToList();

    internal static BridgeImportQueueSummary ImportScanningSummary { get; } =
        ImportSummary with
        {
            FolderScanStatuses = new[]
            {
                new BridgeWatchedFolderScanStatus(
                    ImportRoot,
                    "Incoming",
                    new BridgeFolderScanStatus.Scanning(),
                    OnNetworkVolume: false),
            },
        };

    // A folder read as several releases: the header that says so and offers to
    // read it as one, over the row it produced.
    internal static List<BridgeImportListItem> ImportResolvedItems { get; } = new()
    {
        new BridgeImportListItem.GroupHeader(
            GroupStableKey(ImportGroupKey),
            new BridgeTriageGroup(ImportGroupKey, "Collection", Combinable: true),
            ImportRoot,
            true,
            1),
        new BridgeImportListItem.Candidate(
            CandidateStableKey($"{ImportRoot}/Collection/Release 01"),
            new BridgeTriageRow(
                CandidateKey: $"{ImportRoot}/Collection/Release 01",
                FolderName: "Release 01",
                WatchedFolderPath: ImportRoot,
                DisplayPath: "Collection/Release 01",
                ResolvedBoundaries: new[]
                {
                    new BridgeResolvedFolderReleaseBoundary(
                        ImportGroupKey,
                        BridgeFolderReleaseDecision.KeepAsSeparateReleases,
                        "Collection",
                        "Collection"),
                },
                CombineAncestorKey: null,
                Actionable: true,
                Placement: new BridgeTriagePlacement.Ready(),
                SkipAction: BridgeTriageSkipAction.Skip,
                Matched: null,
                Selectable: true,
                ImportStatus: null,
                MetadataSeed: null),
            IsGroupMember: true),
    };

    internal static BridgeImportQueueSummary ImportResolvedSummary { get; } = Summary(
        pending: 1,
        ready: new[] { ReadyRow("Collection/Release 01") },
        groupKeys: new[] { ImportGroupKey });

    // The stable keys core gives each item kind. Named here so the fixtures
    // carry the same identities a real read would.
    internal static string CandidateStableKey(string candidateKey) =>
        $"candidate:{candidateKey}";

    internal static string GroupStableKey(BridgeFolderReleaseDecisionKey key) =>
        $"group:{key.WatchedFolderPath.Length}{key.WatchedFolderPath}{key.RelativeFolderPath}";

    private static BridgeImportListItem ImportCandidateItem(
        string name,
        string displayPath,
        bool isGroupMember) =>
        new BridgeImportListItem.Candidate(
            CandidateStableKey($"{ImportRoot}/{displayPath}"),
            ImportRow(name, displayPath),
            IsGroupMember: isGroupMember);

    private static BridgeReadyRowRef ReadyRow(string displayPath) => new(
        $"{ImportRoot}/{displayPath}",
        new BridgeMetadataSeed.FileTags(),
        null);

    private static BridgeImportQueueSummary Summary(
        uint pending,
        IReadOnlyList<BridgeReadyRowRef> ready,
        IReadOnlyList<BridgeFolderReleaseDecisionKey> groupKeys) => new(
        Counts: new BridgeTriageTabCounts(Pending: pending, Done: 0, Skipped: 0),
        WatchedFolders: ImportWatchedFolders.ToArray(),
        FolderScanStatuses: Array.Empty<BridgeWatchedFolderScanStatus>(),
        GroupKeys: groupKeys.ToArray(),
        Ready: ready.ToArray(),
        FirstUnidentified: null);

    private static BridgeTriageRow ImportRow(string name, string displayPath) =>
        new(
            CandidateKey: $"{ImportRoot}/{displayPath}",
            FolderName: name,
            WatchedFolderPath: ImportRoot,
            DisplayPath: displayPath,
            ResolvedBoundaries: Array.Empty<BridgeResolvedFolderReleaseBoundary>(),
            CombineAncestorKey: null,
            Actionable: true,
            Placement: new BridgeTriagePlacement.Ready(),
            SkipAction: BridgeTriageSkipAction.Skip,
            Matched: null,
            Selectable: true,
            ImportStatus: null,
            MetadataSeed: null);
}
#endif
