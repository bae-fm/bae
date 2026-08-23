using System;
using System.Linq;
using Avalonia.Controls;
using uniffi.bae_bridge;

namespace Bae.Desktop;

/// <summary>
/// Keeps one control drawing what is in flight for one candidate key.
/// </summary>
/// <remarks>
/// The store keeps no copy of any key's runtime: a running import ticks by the
/// second, and re-projecting the sidebar for each tick would rebuild every row.
/// A control that draws one key attaches this instead — it subscribes while the
/// control is on screen, filters the stream to its own key, and detaches when
/// the control leaves.
///
/// It subscribes first and reads the key's current value second, so a control
/// realized partway through a run shows what is happening rather than waiting
/// for the next change. Both happen on the UI thread in that order, so the read
/// cannot be undone by a change that was already on its way.
/// </remarks>
internal static class CandidateRuntimeObserver
{
    public static void Attach(
        Control control,
        ImportStore store,
        string key,
        Action<BridgeCandidateRuntimeSnapshot?> render)
    {
        void OnChanged(BridgeCandidateRuntimeChange change)
        {
            switch (change)
            {
                case BridgeCandidateRuntimeChange.Updated updated
                    when updated.Key == key:
                    render(updated.Runtime);
                    break;
                case BridgeCandidateRuntimeChange.Removed removed
                    when removed.Key == key:
                    render(null);
                    break;
                case BridgeCandidateRuntimeChange.Reset reset:
                    // Changes were dropped, so this is the whole of what is
                    // running: a key it does not name has nothing running.
                    render(reset.Runtimes
                        .FirstOrDefault(entry => entry.Key == key)?.Runtime);
                    break;
            }
        }

        control.AttachedToVisualTree += (_, _) =>
        {
            store.CandidateRuntimeChanged += OnChanged;
            render(store.CandidateRuntime(key));
        };
        control.DetachedFromVisualTree += (_, _) =>
            store.CandidateRuntimeChanged -= OnChanged;
    }
}
