using System.Collections.Generic;

namespace Bae.Windows;

// One export row's state, projected from the bridge export state at the dialog
// edge so this decision layer stays free of WinRT and uniffi types.
public enum ExportRowKind { Queued, Active, Failed }

// The pure decision layer behind the storage dialog's Exporting section and the
// settings export-location control. The export queue renders from the snapshot
// and actions never optimistically mutate — this only maps counts and state to
// catalog keys and resolves the export destination and the settings Location
// row, so it is unit-tested apart from the WinUI surface that renders it.
public static class ExportQueueModel
{
    // The band's leading label: the generic paused note while paused, else the
    // section title. Mirrors the downloads band's paused/title split.
    public static string BandTitleKey(bool paused) =>
        paused ? "download.paused" : "export.title";

    // The Core-table count labels for the band summary, in exporting/failed/
    // queued order, non-zero counts only. The caller renders each through
    // Loc.Core and joins with " · " — the same composition macOS's summary uses.
    public static List<(string Key, long Count)> SummaryParts(long active, long failed, long queued)
    {
        var parts = new List<(string Key, long Count)>();
        if (active > 0)
        {
            parts.Add(("core.queue.exporting", active));
        }
        if (failed > 0)
        {
            parts.Add(("core.queue.failed", failed));
        }
        if (queued > 0)
        {
            parts.Add(("core.queue.queued", queued));
        }
        return parts;
    }

    // Retry-failed is offered only when something is failed.
    public static bool RetryEnabled(long failedCount) => failedCount > 0;

    // The pause/resume toggle label: resume while paused, else pause.
    public static string PauseToggleKey(bool paused) =>
        paused ? "outbox.resume" : "outbox.pause";

    // A row's state label key. Queued and failed reuse the download-state keys
    // (their values are generic); active is export-specific because it carries
    // the percent.
    public static string StateKey(ExportRowKind kind) => kind switch
    {
        ExportRowKind.Active => "export.state.exporting",
        ExportRowKind.Failed => "download.state.failed",
        _ => "download.state.queued",
    };

    // Progress percent clamped to 0..100 for the bar and the label.
    public static int ClampPercent(int percent) =>
        percent < 0 ? 0 : percent > 100 ? 100 : percent;

    // The destination for a release export: the configured fixed directory, or
    // null meaning "prompt for a folder". A fixed location with a blank
    // directory degrades to prompting rather than exporting to nowhere.
    public static string? DestinationFor(bool isFixed, string? fixedDir) =>
        isFixed && !string.IsNullOrWhiteSpace(fixedDir) ? fixedDir : null;

    // The path shown in the settings Location row: the configured folder when
    // fixed, else the last-remembered folder, else null (the caller shows the
    // no-folder placeholder).
    public static string? LocationRowPath(bool isFixed, string? fixedDir, string? remembered) =>
        isFixed ? fixedDir : remembered;

    // Selecting "save to a folder" prompts when nothing is remembered;
    // otherwise the remembered folder is applied directly.
    public static bool FixedSelectionNeedsPrompt(string? remembered) =>
        string.IsNullOrWhiteSpace(remembered);
}
