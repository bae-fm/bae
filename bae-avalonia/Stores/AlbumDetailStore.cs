using System;

namespace Bae.Desktop;

/// <summary>
/// Owns the live album-detail query for the browser's current expansion. A new
/// album replaces the prior subscription, and callbacks queued by an evicted
/// subscription cannot mutate the current value.
/// </summary>
internal sealed class AlbumDetailStore : IDisposable
{
    private readonly LibraryService _library;
    private readonly Action<Action> _dispatch;
    private readonly Action<Exception> _onError;
    private IDisposable? _subscription;
    private long _generation;

    public string? AlbumId { get; private set; }
    public AlbumDetail? Detail { get; private set; }
    public bool HasValue { get; private set; }
    public event Action? Changed;

    public AlbumDetailStore(
        LibraryService library,
        Action<Action> dispatch,
        Action<Exception> onError)
    {
        _library = library;
        _dispatch = dispatch;
        _onError = onError;
    }

    public void Select(string albumId)
    {
        if (AlbumId == albumId)
        {
            return;
        }

        _subscription?.Dispose();
        var generation = ++_generation;
        AlbumId = albumId;
        Detail = null;
        HasValue = false;
        Changed?.Invoke();
        _subscription = _library.SubscribeAlbumDetail(
            albumId,
            detail => _dispatch(() => ApplyValue(generation, albumId, detail)),
            error => _dispatch(() => ApplyError(generation, albumId, error)));
    }

    public void Clear(string albumId)
    {
        if (AlbumId != albumId)
        {
            return;
        }

        _subscription?.Dispose();
        _subscription = null;
        _generation += 1;
        AlbumId = null;
        Detail = null;
        HasValue = false;
        Changed?.Invoke();
    }

    private bool IsCurrent(long generation, string albumId) =>
        generation == _generation && AlbumId == albumId;

    private void ApplyValue(long generation, string albumId, AlbumDetail? detail)
    {
        if (!IsCurrent(generation, albumId))
        {
            return;
        }
        Detail = detail;
        HasValue = true;
        Changed?.Invoke();
    }

    private void ApplyError(long generation, string albumId, Exception error)
    {
        if (!IsCurrent(generation, albumId))
        {
            return;
        }
        _onError(error);
    }

    public void Dispose()
    {
        _subscription?.Dispose();
        _subscription = null;
        _generation += 1;
    }
}
