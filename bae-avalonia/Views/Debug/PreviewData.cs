#if DEBUG
using System;
using System.Collections.Generic;
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

    internal static BridgeTriageQueue ImportQueue { get; } = new(
        Sections: new[]
        {
            new BridgeTriageSection(
                BridgeTriageTab.Ready,
                ImportRoot,
                new BridgeTriageGroup(
                    ImportGroupKey,
                    "Collection"),
                new BridgeTriageEntry[]
                {
                    new BridgeTriageEntry.Candidate(
                        $"{ImportRoot}/Collection/Release 01",
                        ImportRow("Release 01", "Collection/Release 01")),
                    new BridgeTriageEntry.Candidate(
                        $"{ImportRoot}/Collection/Release 02",
                        ImportRow("Release 02", "Collection/Release 02")),
                }),
            new BridgeTriageSection(
                BridgeTriageTab.Ready,
                ImportRoot,
                null,
                new BridgeTriageEntry[]
                {
                    new BridgeTriageEntry.Candidate(
                        $"{ImportRoot}/Release 03",
                        ImportRow("Release 03", "Release 03")),
                }),
            new BridgeTriageSection(
                BridgeTriageTab.NeedsYou,
                ImportRoot,
                null,
                new BridgeTriageEntry[]
                {
                    new BridgeTriageEntry.Boundary(
                        $"{ImportRoot}/Archive/Box",
                        new BridgeFolderReleaseBoundary(
                            new BridgeFolderReleaseDecisionKey(
                                ImportRoot,
                                "Archive/Box"),
                            "Box",
                            "Archive/Box",
                            2,
                            new[]
                            {
                                new BridgeFolderReleaseTreeRow(
                                    "Part 01",
                                    "Part 01",
                                    0,
                                    new BridgeFolderReleaseTreeRowKind.Candidate(9, "FLAC"),
                                    new BridgeFolderReleaseDecisionKey(ImportRoot, "Archive/Box/Part 01"),
                                    new[] { new BridgeFolderReleaseDecisionKey(ImportRoot, "Archive/Box") }),
                                new BridgeFolderReleaseTreeRow(
                                    "Part 02",
                                    "Part 02",
                                    0,
                                    new BridgeFolderReleaseTreeRowKind.Candidate(11, "FLAC"),
                                    new BridgeFolderReleaseDecisionKey(ImportRoot, "Archive/Box/Part 02"),
                                    new[] { new BridgeFolderReleaseDecisionKey(ImportRoot, "Archive/Box") }),
                                new BridgeFolderReleaseTreeRow(
                                    "Scans",
                                    "Scans",
                                    0,
                                    new BridgeFolderReleaseTreeRowKind.Folder(),
                                    new BridgeFolderReleaseDecisionKey(ImportRoot, "Archive/Box/Scans"),
                                    new[] { new BridgeFolderReleaseDecisionKey(ImportRoot, "Archive/Box") }),
                                new BridgeFolderReleaseTreeRow(
                                    "Booklet",
                                    "Scans/Booklet",
                                    1,
                                    new BridgeFolderReleaseTreeRowKind.Folder(),
                                    new BridgeFolderReleaseDecisionKey(ImportRoot, "Archive/Box/Scans/Booklet"),
                                    new[]
                                    {
                                        new BridgeFolderReleaseDecisionKey(ImportRoot, "Archive/Box"),
                                        new BridgeFolderReleaseDecisionKey(ImportRoot, "Archive/Box/Scans"),
                                    }),
                            }))
                }),
        },
        Counts: new BridgeTriageTabCounts(
            Ready: 3,
            NeedsYou: 1,
            Done: 0,
            Skipped: 0),
        FolderScanStatuses: Array.Empty<BridgeWatchedFolderScanStatus>());

    internal static BridgeTriageQueue ImportScanningQueue { get; } =
        ImportQueue with
        {
            FolderScanStatuses = new[]
            {
                new BridgeWatchedFolderScanStatus(
                    ImportRoot,
                    "Incoming",
                    new BridgeFolderScanStatus.Scanning()),
            },
        };

    internal static BridgeTriageQueue ImportResolvedQueue { get; } = new(
        Sections: new[]
        {
            new BridgeTriageSection(
                BridgeTriageTab.Ready,
                ImportRoot,
                null,
                new BridgeTriageEntry[]
                {
                    new BridgeTriageEntry.Candidate(
                        $"{ImportRoot}/Collection/Release 01",
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
                            Matched: null,
                            Selectable: true,
                            ImportStatus: null))
                }),
        },
        Counts: new BridgeTriageTabCounts(
            Ready: 1,
            NeedsYou: 0,
            Done: 0,
            Skipped: 0),
        FolderScanStatuses: Array.Empty<BridgeWatchedFolderScanStatus>());

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
            Matched: null,
            Selectable: true,
            ImportStatus: null);
}
#endif
