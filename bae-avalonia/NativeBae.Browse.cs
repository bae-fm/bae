using uniffi.bae_bridge;

namespace Bae.Desktop;

// The library grids' pages, each read through a browse subscription asked for
// just that page's window.
internal static partial class NativeBae
{
    internal static IDisposable SubscribeAlbumPage(
        AppHandle handle,
        ulong offset,
        ulong limit,
        IReadOnlyList<SortCriterion<AlbumSortField>> criteria,
        Action<IReadOnlyList<Album>, int> onValue,
        Action<Exception> onError)
    {
        var subscription = handle.SubscribeAlbumBrowse(ToBridge(criteria));
        return BrowsedPage.Start(
            subscription,
            () => subscription.SetWindows([new BridgeLibraryPageWindow(offset, limit)]),
            async () =>
            {
                var snapshot = await subscription.Next();
                return (snapshot.Windows.SelectMany(window => window.Rows).Select(row => new Album(row)).ToList(),
                    checked((int)snapshot.TotalCount));
            },
            subscription.Cancel,
            onValue,
            onError);
    }

    internal static IDisposable SubscribeComposerPage(
        AppHandle handle,
        ulong offset,
        ulong limit,
        IReadOnlyList<SortCriterion<ComposerSortField>> criteria,
        Action<IReadOnlyList<ComposerSummary>, int> onValue,
        Action<Exception> onError)
    {
        var subscription = handle.SubscribeComposerBrowse(ToBridge(criteria));
        return BrowsedPage.Start(
            subscription,
            () => subscription.SetWindows([new BridgeLibraryPageWindow(offset, limit)]),
            async () =>
            {
                var snapshot = await subscription.Next();
                return (snapshot.Windows.SelectMany(window => window.Rows).Select(row => new ComposerSummary(row)).ToList(),
                    checked((int)snapshot.TotalCount));
            },
            subscription.Cancel,
            onValue,
            onError);
    }

    internal static IDisposable SubscribeArtistPage(
        AppHandle handle,
        ulong offset,
        ulong limit,
        IReadOnlyList<SortCriterion<ArtistSortField>> criteria,
        Action<IReadOnlyList<ArtistSummary>, int> onValue,
        Action<Exception> onError)
    {
        var subscription = handle.SubscribeArtistBrowse(ToBridge(criteria));
        return BrowsedPage.Start(
            subscription,
            () => subscription.SetWindows([new BridgeLibraryPageWindow(offset, limit)]),
            async () =>
            {
                var snapshot = await subscription.Next();
                return (snapshot.Windows.SelectMany(window => window.Rows).Select(row => new ArtistSummary(row)).ToList(),
                    checked((int)snapshot.TotalCount));
            },
            subscription.Cancel,
            onValue,
            onError);
    }

    /// <summary>One page of a library list, read through a browse subscription
    /// asked for just this page's window. Disposing cancels the pending read
    /// and frees the subscription.</summary>
    private sealed class BrowsedPage : IDisposable
    {
        private readonly object _subscription;
        private readonly Func<Task> _cancel;

        private BrowsedPage(object subscription, Func<Task> cancel)
        {
            _subscription = subscription;
            _cancel = cancel;
        }

        public static BrowsedPage Start<TRow>(
            object subscription,
            Action setWindow,
            Func<Task<(IReadOnlyList<TRow> Rows, int Total)>> next,
            Func<Task> cancel,
            Action<IReadOnlyList<TRow>, int> onValue,
            Action<Exception> onError)
        {
            var page = new BrowsedPage(subscription, cancel);
            try
            {
                setWindow();
            }
            catch (BridgeException error)
            {
                onError(new PageLoadException(error.Message));
                return page;
            }
            _ = Task.Run(async () =>
            {
                while (true)
                {
                    (IReadOnlyList<TRow> Rows, int Total) value;
                    try
                    {
                        value = await next();
                    }
                    catch (BridgeException.Cancelled)
                    {
                        return;
                    }
                    catch (BridgeException error)
                    {
                        onError(new PageLoadException(error.Message));
                        return;
                    }
                    onValue(value.Rows, value.Total);
                }
            });
            return page;
        }

        public void Dispose()
        {
            // Cancelling settles the pending read so the loop ends; freeing the
            // object afterwards releases what core held for it.
            _ = _cancel();
            (_subscription as IDisposable)?.Dispose();
        }
    }
}
