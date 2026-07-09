using System;
using System.Collections.Generic;
using System.Linq;

namespace Bae.Windows;

// App-lifetime, UI-thread store of in-flight release transfers (pin/unpin/
// manage/unmanage). Wraps a StorageTransferOverlay and raises Changed so the
// storage dialog rows and the album-detail storage band re-render their in-flight
// indicators as core's ReleaseTransferProgress / ReleaseTransferEnded events
// arrive. Cleared when the library closes or switches, like the other per-library
// stores, so a transfer from the prior library never bleeds into the next one.
internal sealed class TransferProgressStore
{
    private readonly StorageTransferOverlay _overlay = new();

    public event Action? Changed;

    public void Apply(string releaseId, string actionToken)
    {
        _overlay.Apply(releaseId, actionToken);
        Changed?.Invoke();
    }

    public void Clear(string releaseId)
    {
        if (_overlay.Clear(releaseId))
        {
            Changed?.Invoke();
        }
    }

    public string? TokenFor(string releaseId) => _overlay.TokenFor(releaseId);

    public IReadOnlyCollection<string> ActiveReleaseIds => _overlay.ActiveReleaseIds;

    // Drop every in-flight entry on library close/switch.
    public void Reset()
    {
        var active = _overlay.ActiveReleaseIds.ToList();
        if (active.Count == 0)
        {
            return;
        }
        foreach (var releaseId in active)
        {
            _overlay.Clear(releaseId);
        }
        Changed?.Invoke();
    }
}
