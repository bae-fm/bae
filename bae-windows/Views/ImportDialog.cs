using System;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;

namespace Bae.Windows;

// The import dialog: a folder scan source plus the live candidate list bound to
// the import store. Clicking an unidentified candidate kicks off auto-identify;
// once it has a result, clicking opens the picker. Shared by the toolbar import
// button and the folder-drop handler.
internal sealed class ImportDialog
{
    private readonly SessionStore _session;
    private readonly Func<XamlRoot?> _xamlRoot;
    private readonly Func<IntPtr> _windowHandle;
    private readonly ImportStore _import;
    private readonly ImportPickerDialog _picker;

    // True while the dialog is showing. The window-drop handler reads it to avoid
    // opening a second ContentDialog over an already-open import dialog.
    public bool IsOpen { get; private set; }

    public ImportDialog(
        SessionStore session,
        Func<XamlRoot?> xamlRoot,
        Func<IntPtr> windowHandle,
        ImportStore import,
        ImportPickerDialog picker)
    {
        _session = session;
        _xamlRoot = xamlRoot;
        _windowHandle = windowHandle;
        _import = import;
        _picker = picker;
    }

    public async System.Threading.Tasks.Task Show()
    {
        if (_session.CurrentHandleOrNull() == null)
        {
            return;
        }
        _import.RefreshCandidates();

        var scanButton = new Button { Content = Loc.Chrome("import.choose_folder") };
        var status = new TextBlock
        {
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
            TextWrapping = TextWrapping.Wrap,
            Visibility = Visibility.Collapsed,
        };
        // The candidate refresh reports its no-releases line through the store;
        // render it into the status block without changing visibility (a scan
        // makes the block visible), matching the prior in-place update.
        void RenderScanStatus() => status.Text = _import.CandidatesStatusText;
        _import.CandidatesRefreshed += RenderScanStatus;

        var list = new ListView
        {
            ItemsSource = _import.Candidates,
            SelectionMode = ListViewSelectionMode.None,
            IsItemClickEnabled = true,
            MaxHeight = 320,
        };

        // Each row builds imperatively: the candidate's one-line summary plus a
        // signals badge row. ContainerContentChanging fires per recycled container
        // (re-firing when UpdateCandidate swaps in a fresh instance), so the badges
        // refresh as identify events land.
        list.ContainerContentChanging += (_, args) =>
        {
            if (args.InRecycleQueue || args.Item is not ImportCandidate candidate)
            {
                return;
            }

            args.ItemContainer.Content = BuildCandidateRow(candidate);
            args.Handled = true;
        };

        // First click on an unidentified candidate kicks off auto-identification;
        // once it has a result, clicking opens the import dialog (auto matches
        // plus a manual-search fallback). The row reflects status from the
        // candidate snapshot.
        list.ItemClick += async (_, args) =>
        {
            if (args.ClickedItem is not ImportCandidate candidate)
            {
                return;
            }

            if (string.IsNullOrEmpty(candidate.Status))
            {
                _ = _import.AutoIdentify(candidate.Key, candidate.FolderPath);
            }
            else
            {
                await _picker.Show(candidate);
            }
        };

        // The picker needs the app window's handle in an unpackaged app. Adding
        // a watched folder starts scanning; invalidations refresh the candidates.
        scanButton.Click += async (_, _) =>
        {
            var picker = new global::Windows.Storage.Pickers.FolderPicker();
            picker.FileTypeFilter.Add("*");
            WinRT.Interop.InitializeWithWindow.Initialize(picker, _windowHandle());
            var folder = await picker.PickSingleFolderAsync();
            if (folder is null)
            {
                return;
            }

            status.Text = Loc.Chrome("import.scanning");
            status.Visibility = Visibility.Visible;
            var path = folder.Path;
            var (current, error) = await _import.ScanFolder(path);
            if (!current)
            {
                return;
            }
            if (error is not null)
            {
                status.Text = error;
            }
            else
            {
                _import.RefreshCandidates();
            }
        };

        var content = new StackPanel { Spacing = 8, Width = 420 };
        content.Children.Add(scanButton);
        content.Children.Add(status);
        content.Children.Add(list);

        var dialog = new ContentDialog
        {
            Title = Loc.Chrome("import.title"),
            Content = content,
            CloseButtonText = Loc.Chrome("action.close"),
            XamlRoot = _xamlRoot(),
        };

        IsOpen = true;
        try
        {
            await dialog.ShowAsync();
        }
        finally
        {
            IsOpen = false;
            _import.CandidatesRefreshed -= RenderScanStatus;
        }
    }

    // One candidate row: the summary line, then a signals badge row underneath
    // (omitted until the first toolbar event lands). Core pre-shapes every badge;
    // this iterates and renders.
    private StackPanel BuildCandidateRow(ImportCandidate candidate)
    {
        var row = new StackPanel { Spacing = 4 };
        row.Children.Add(new TextBlock
        {
            Text = candidate.ToString(),
            TextWrapping = TextWrapping.Wrap,
        });

        if (candidate.Signals.Count > 0)
        {
            row.Children.Add(SignalBadgeRow.Build(
                candidate.Signals,
                (kind, value) =>
                {
                    if (_session.CurrentHandleOrNull() != null)
                    {
                        _ = _import.ToggleSignal(candidate.Key, kind, value);
                    }
                },
                () =>
                {
                    if (_session.CurrentHandleOrNull() != null)
                    {
                        _ = _import.RerunIdentify(candidate.Key);
                    }
                }));
        }

        return row;
    }
}
