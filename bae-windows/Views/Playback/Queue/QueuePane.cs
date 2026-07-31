using System;
using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.Collections.Specialized;
using System.Globalization;
using System.Linq;
using System.Runtime.InteropServices.WindowsRuntime;
using System.Threading.Tasks;
using Microsoft.UI;
using Microsoft.UI.Text;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Microsoft.UI.Xaml.Data;
using Microsoft.UI.Xaml.Media;
using uniffi.bae_bridge;
using Windows.ApplicationModel.DataTransfer;
using Windows.Foundation;
using Windows.UI;

namespace Bae.Windows;

// The queue side pane: a non-modal surface hosting the manual "Up Next" lane and
// the playback context as ONE virtualized, reorderable ListView (never a ListView
// inside a StackPanel inside a ScrollViewer — that combination hands the ListView
// unbounded height, so ItemsStackPanel realizes every row instead of
// virtualizing). Non-modal so the album grid behind it can source a drag while
// the pane stays open — album cards drop into the manual lane at a chosen index.
// Reads the lanes from the playback store and rebuilds the list on its
// QueueChanged while visible; each row skips on click, removes on right-tap or its
// remove button, and reorders on drag (forwarded to core by entry id). Clear
// empties only the manual lane. External drops land only in the manual lane; the
// context lane rejects them. The context lane is library-scaled and only partly
// resolved: it pages further entries in via ISupportIncrementalLoading as the list
// scrolls toward its end, revision-checked against the store so a reply for a
// superseded queue state is dropped.
//
// The header and the now-playing card are fixed chrome that survive list rebuilds:
// the shell (header / card / list slot) is built once when the pane opens, and a
// QueueChanged only swaps a fresh list into the slot. The card renders the same
// now-playing track the bar shows, driven by the store's now-playing and position
// events (subscribed for the pane's whole life), not by QueueChanged.
internal sealed partial class QueuePane
{
    private readonly SessionStore _session;
    private readonly ImageStore _images;
    private readonly LibraryService _library;
    private readonly QueueService _queueService;
    private readonly PlaybackStore _playback;
    private readonly Border _host;
    private readonly Action<string> _onError;

    // Shell chrome, built once per open and held so events update it in place. Null
    // while the pane is closed, which is the guard the now-playing handlers check
    // before touching any visual.
    private ContentControl? _listSlot;
    private Border? _card;
    private Image? _cardCover;
    private TextBlock? _cardTitle;
    private TextBlock? _cardArtist;
    private TextBlock? _cardElapsed;
    private ColumnDefinition? _cardProgressFill;
    private ColumnDefinition? _cardProgressRest;

    // The latest now-playing snapshot the store has emitted, cached for the whole
    // pane life so opening the pane can render the card immediately (no event fires
    // just because the pane opened). Null while playback is stopped.
    private NowPlayingBarTrack? _nowPlaying;
    private PlaybackPositionRender? _position;

    // The whole second the card's elapsed readout last rendered, so position ticks
    // (several per second) collapse to one update per second — it is a readout, not
    // a scrubber.
    private long _renderedSecond = -1;

    public QueuePane(
        SessionStore session,
        ImageStore images,
        LibraryService library,
        QueueService queueService,
        PlaybackStore playback,
        Border host,
        Action<string> onError)
    {
        _session = session;
        _images = images;
        _library = library;
        _queueService = queueService;
        _playback = playback;
        _host = host;
        _onError = onError;

        // The card follows the same now-playing state the bar renders. Subscribed
        // for the pane's whole life (cheap) so the cache is warm when the pane
        // opens; the handlers no-op on the visuals while the pane is closed.
        _playback.NowPlayingChanged += OnNowPlayingChanged;
        _playback.PlaybackStopped += OnPlaybackStopped;
        _playback.PositionChanged += OnPositionChanged;
    }

    public bool IsOpen => _host.Visibility == Visibility.Visible;

    // Wire the queue button in the now-playing bar: a click (or its Ctrl+Shift+Q
    // accelerator) toggles the pane, and a card dropped on it appends the album's
    // tracks to the end of the manual lane — whether or not the pane is open. The
    // caption names the append action, distinct from the in-pane drops' insert
    // preview; e.Handled keeps the window's folder-drop handler from overwriting
    // the accepted operation for a non-file drag.
    public void AttachToggleButton(Button button)
    {
        button.Click += (_, _) => Toggle();
        button.AllowDrop = true;
        button.DragOver += (_, e) =>
        {
            if (_session.CurrentHandleOrNull() == null || !e.DataView.Contains(StandardDataFormats.Text))
            {
                return;
            }
            e.AcceptedOperation = DataPackageOperation.Copy;
            if (e.DragUIOverride is not null)
            {
                e.DragUIOverride.Caption = Loc.Chrome("menu.add_to_queue");
            }
            e.Handled = true;
        };
        button.Drop += async (_, e) =>
        {
            if (_session.CurrentHandleOrNull() == null || !e.DataView.Contains(StandardDataFormats.Text))
            {
                return;
            }
            e.Handled = true;
            await HandleButtonAppendDrop(e);
        };
    }

    // Show the pane if hidden, hide it if shown. Bound to the queue button and its
    // Ctrl+Shift+Q accelerator.
    public void Toggle()
    {
        if (IsOpen)
        {
            Hide();
        }
        else
        {
            Show();
        }
    }

    public void Hide()
    {
        if (!IsOpen)
        {
            return;
        }
        _playback.QueueChanged -= Rebuild;
        _host.Child = null;
        _listSlot = null;
        _card = null;
        _cardCover = null;
        _cardTitle = null;
        _cardArtist = null;
        _cardElapsed = null;
        _cardProgressFill = null;
        _cardProgressRest = null;
        _host.Visibility = Visibility.Collapsed;
    }

    private void Show()
    {
        if (_session.CurrentHandleOrNull() == null)
        {
            return;
        }
        _host.Child = BuildShell();
        Rebuild();
        RenderCard();
        _playback.QueueChanged += Rebuild;
        _host.Visibility = Visibility.Visible;
    }

    // The pane's fixed chrome: a header, the now-playing card, and a slot the list
    // is swapped into on each rebuild. Built once per open.
    private Grid BuildShell()
    {
        var shell = new Grid();
        shell.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        shell.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        shell.RowDefinitions.Add(new RowDefinition { Height = new GridLength(1, GridUnitType.Star) });

        var header = BuildHeader();
        Grid.SetRow(header, 0);
        shell.Children.Add(header);

        var card = BuildNowPlayingCard();
        Grid.SetRow(card, 1);
        shell.Children.Add(card);

        _listSlot = new ContentControl
        {
            HorizontalContentAlignment = HorizontalAlignment.Stretch,
            VerticalContentAlignment = VerticalAlignment.Stretch,
        };
        Grid.SetRow(_listSlot, 2);
        shell.Children.Add(_listSlot);

        return shell;
    }

    // Rebuild the pane's list from the current lanes. Safe to run mid-drag: core
    // emits QueueChanged only after a mutation lands, and the list is rebuilt
    // whole between events. QueueChanged is also the invalidation signal for any
    // in-flight incremental-load page fetch on the PREVIOUS collection instance:
    // this always constructs a fresh QueuePaneRowCollection, so a stale reply lands
    // on an abandoned instance nobody renders. The header and card are untouched.
    private void Rebuild()
    {
        var manual = _playback.ManualQueue;
        var context = _playback.Context;
        var revision = _playback.Revision;

        var rows = new QueuePaneRowCollection(_queueService, revision, contextTotal: context?.UpcomingTotal ?? 0);

        // "Up Next" labels the manual lane only when it has rows (an empty lane
        // shows its drop card without a heading, matching macOS).
        if (manual.Count == 0)
        {
            rows.Add(new EmptyManualRow());
        }
        else
        {
            rows.Add(new SectionHeaderRow(QueueLane.Manual, Loc.Chrome("queue.section.up_next"), Shuffled: null));
            foreach (var entry in manual)
            {
                rows.Add(new EntryRow(QueueLane.Manual, entry));
            }
            rows.Add(new TrailingDropRow());
        }

        if (context is { UpcomingTotal: > 0 } ctx)
        {
            rows.Add(new SectionHeaderRow(QueueLane.Context, ContextSectionTitle(ctx), ctx.Shuffled));
            foreach (var entry in ctx.Upcoming)
            {
                rows.Add(new EntryRow(QueueLane.Context, entry));
            }
            rows.MarkInitialContextLoad((ulong)ctx.Upcoming.Length);
        }

        _listSlot!.Content = BuildQueueList(rows, manual);
    }

    // The context section's title: the whole library, or "Playing From" a release —
    // with the release title appended ("Playing From · {title}") when the context
    // carries one. The localized words are the UI's; the composition mirrors macOS.
    private static string ContextSectionTitle(BridgePlaybackContext context)
    {
        if (context.Kind == BridgePlaybackSourceKind.Library)
        {
            return Loc.Chrome("queue.section.your_library");
        }
        var playingFrom = Loc.Chrome("queue.section.playing_from");
        return string.IsNullOrEmpty(context.SourceTitle)
            ? playingFrom
            : $"{playingFrom} · {context.SourceTitle}";
    }

    // The pane header: the "queue" title and a close button that hides the pane.
    // Each lane clears itself from its own section header.
    private Grid BuildHeader()
    {
        var title = new TextBlock
        {
            Text = Loc.Chrome("queue.title"),
            FontSize = 22,
            FontWeight = FontWeights.ExtraBold,
            VerticalAlignment = VerticalAlignment.Center,
        };

        var close = new Button
        {
            // Segoe MDL2 Assets "ChromeClose" glyph (U+E711).
            Content = new FontIcon { Glyph = "\uE711", FontSize = 14 },
            Background = new SolidColorBrush(Colors.Transparent),
            BorderThickness = new Thickness(0),
            VerticalAlignment = VerticalAlignment.Center,
        };
        AutomationName(close, "action.close");
        close.Click += (_, _) => Hide();

        var grid = new Grid { Padding = new Thickness(20, 18, 12, 12) };
        grid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        grid.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        Grid.SetColumn(title, 0);
        Grid.SetColumn(close, 1);
        grid.Children.Add(title);
        grid.Children.Add(close);
        return grid;
    }

}
