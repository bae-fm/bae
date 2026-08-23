using System;
using uniffi.bae_bridge;

namespace Bae.Desktop;

/// <summary>
/// The import tab's subscriptions. The list is one reconfigurable object the
/// caller drives; the candidate under the pane and the runtime of every key in
/// flight arrive through callbacks.
/// </summary>
internal static partial class NativeBae
{
    /// <summary>The import tab's list: one object per list, reconfigured by
    /// view and by window. The caller drives its Next loop.</summary>
    internal static IImportListSubscription SubscribeImportList(
        AppHandle handle,
        BridgeImportListView view) =>
        handle.SubscribeImportList(view);

    /// <summary>One candidate as the pane reads it, and every later read of
    /// it. A null value means the key names no scanned folder any more.</summary>
    internal static LiveSubscription SubscribeImportCandidate(
        AppHandle handle,
        string candidateKey,
        Action<BridgeImportCandidateDetail?> onValue,
        Action<Exception> onError) =>
        handle.SubscribeImportCandidate(
            candidateKey,
            new ImportCandidateSink(onValue, onError));

    private sealed class ImportCandidateSink(
        Action<BridgeImportCandidateDetail?> onValue,
        Action<Exception> onError) : ImportCandidateCallback
    {
        public void OnValue(BridgeImportCandidateDetail? value) => onValue(value);
        public void OnError(BridgeException error) => onError(error);
    }

    /// <summary>What is in flight for one key right now — the read a control
    /// does once when it appears, after it has subscribed to the changes.
    /// </summary>
    internal static BridgeCandidateRuntimeSnapshot? CandidateRuntime(
        AppHandle handle,
        string candidateKey) =>
        handle.CandidateRuntime(candidateKey);

    /// <summary>What every candidate has in flight: one change per running key
    /// on subscribe, then one per change as runs advance.</summary>
    internal static LiveSubscription SubscribeCandidateRuntime(
        AppHandle handle,
        Action<BridgeCandidateRuntimeChange> onChange) =>
        handle.SubscribeCandidateRuntime(new CandidateRuntimeSink(onChange));

    private sealed class CandidateRuntimeSink(
        Action<BridgeCandidateRuntimeChange> onChange) : CandidateRuntimeCallback
    {
        public void OnChange(BridgeCandidateRuntimeChange change) => onChange(change);
    }
}
