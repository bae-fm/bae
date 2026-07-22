using System.Collections.Generic;

namespace Bae.Windows;

// One export row's state, projected from the bridge export state at the dialog
// edge so this decision layer stays free of WinRT and uniffi types.
public enum OutputRowKind { Queued, Active, Failed }

// Whether an output row is a verbatim export or a preset save. Projected from
// the bridge `BridgeOutputKind` at the dialog edge, like `OutputRowKind`.
public enum OutputKind { Export, Save }

// The pure decision layer behind the storage dialog's Exporting section. The
// export queue renders from the snapshot and actions never optimistically
// mutate — this only maps counts and state to catalog keys, so it is
// unit-tested apart from the WinUI surface that renders it.
public static class OutputQueueModel
{
    // The band's leading label: the generic paused note while paused, else the
    // section title. Mirrors the downloads band's paused/title split.
    public static string BandTitleKey(bool paused) =>
        paused ? "download.paused" : "output.title";

    // Retry-failed is offered only when something is failed.
    public static bool RetryEnabled(long failedCount) => failedCount > 0;

    // The pause/resume toggle label: resume while paused, else pause.
    public static string PauseToggleKey(bool paused) =>
        paused ? "outbox.resume" : "outbox.pause";

    // A row's state label key. Queued and failed reuse the download-state keys
    // (their values are generic); the active key carries the percent and differs
    // by output kind — exporting vs saving.
    public static string StateKey(OutputRowKind kind, OutputKind outputKind) => kind switch
    {
        OutputRowKind.Active => outputKind == OutputKind.Save
            ? "export.state.saving"
            : "export.state.exporting",
        OutputRowKind.Failed => "download.state.failed",
        _ => "download.state.queued",
    };

    // Progress percent clamped to 0..100 for the bar and the label.
    public static int ClampPercent(int percent) =>
        percent < 0 ? 0 : percent > 100 ? 100 : percent;
}
