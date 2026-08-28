using System;
using System.Threading.Tasks;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Layout;
using Avalonia.Markup.Xaml.MarkupExtensions;
using Avalonia.Media;
using uniffi.bae_bridge;
namespace Bae.Desktop;

/// <summary>
/// The mapping pane's commit bar: what is still unanswered, where the release
/// will be stored, and the action that writes it into the library — plus the
/// two banners that state what a commit already did, a folder that is in the
/// library already and an import of it that failed.
/// </summary>
internal sealed partial class ImportMappingPane
{
    // Storage: cloud against local, and the pin that rides with a cloud import.
    // Only offered when the library has a cloud home.
    private bool _storageCloud = true;
    private bool _storagePinned = true;

    // What the last commit refused with. A string and not the control that
    // shows it: the pane rebuilds its tree on every render, and a control held
    // across renders would be added to a second parent on the next one.
    private string _commitError = string.Empty;

    private TextBlock? _unansweredText;

    /// <summary>The selected metadata card's foot: what is still unanswered, storage,
    /// and the Import action — the commit lives on the card that states what
    /// will be committed.</summary>
    private Control BuildCommitRow()
    {
        var (settingsCurrent, settings) = _app.Settings.GetSettings();
        var hasCloudHome = settingsCurrent && settings.HasCloudHome;

        var counts = new StackPanel { Spacing = 1, VerticalAlignment = VerticalAlignment.Center };
        _unansweredText = ImportPaneUi.Cell(string.Empty, secondary: true);
        counts.Children.Add(_unansweredText);
        RefreshCommitCounts();

        var storage = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Spacing = 10,
            VerticalAlignment = VerticalAlignment.Center,
        };
        if (hasCloudHome)
        {
            var cloud = new CheckBox { Content = Loc.Chrome("import.storage.cloud"), IsChecked = _storageCloud };
            var pinned = new CheckBox
            {
                Content = Loc.Chrome("import.storage.pinned"),
                IsChecked = _storagePinned,
                IsVisible = _storageCloud,
            };
            cloud.IsCheckedChanged += (_, _) =>
            {
                _storageCloud = cloud.IsChecked == true;
                pinned.IsVisible = _storageCloud;
            };
            pinned.IsCheckedChanged += (_, _) => _storagePinned = pinned.IsChecked == true;
            storage.Children.Add(cloud);
            storage.Children.Add(pinned);
        }

        // While the import runs, what stood here is the run itself: the same
        // step, percent and bar the candidate's row shows, from the same
        // component reading the same signal. There is nothing to press — the
        // commit already happened — so the action's place is where the answer
        // to "how far along is it?" belongs.
        Control import;
        if (EffectiveRowStatus.Kind == "importing" && _key is { } running)
        {
            import = ImportProgressLine.Build(_import, running);
            import.MinWidth = 200;
            import.VerticalAlignment = VerticalAlignment.Center;
        }
        else
        {
            // Nothing here disables the commit. The counts are stated; the one
            // refusal left in the whole import is audio that will not decode,
            // and core raises that.
            var button = DialogUi.Primary(Loc.Chrome("action.import"));
            button.Click += async (_, _) => await Commit();
            import = button;
        }

        var row = new Grid { ColumnDefinitions = new ColumnDefinitions("*,Auto,Auto"), ColumnSpacing = 12 };
        Grid.SetColumn(counts, 0);
        Grid.SetColumn(storage, 1);
        Grid.SetColumn(import, 2);
        row.Children.Add(counts);
        row.Children.Add(storage);
        row.Children.Add(import);

        var column = new StackPanel { Spacing = 6 };
        column.Children.Add(row);
        if (_commitError.Length > 0)
        {
            var refusal = DialogUi.Danger();
            refusal.Text = _commitError;
            column.Children.Add(refusal);
        }
        return column;
    }

    // What is still unanswered, restated from the table core answered with.
    private void RefreshCommitCounts()
    {
        if (_candidate?.Detail is null || _unansweredText is null)
        {
            return;
        }
        var mapping = _candidate.Mapping;
        var unanswered = mapping.UnansweredCount();
        _unansweredText.Text = unanswered == 0
            ? string.Empty
            : Loc.Core("ui.import.commit.unanswered", "count", (long)unanswered);
        _unansweredText.IsVisible = unanswered > 0;
    }

    // Commit the candidate. Nothing about the release is sent: the pick, the
    // metadata typed over it, the corrected rows and the chosen cover are all
    // stored under the candidate, so the commit consumes the very values this
    // pane drew.
    private async Task Commit()
    {
        if (_key is not { } key)
        {
            return;
        }
        var (settingsCurrent, settings) = _app.Settings.GetSettings();
        var cloud = settingsCurrent && settings.HasCloudHome && _storageCloud;

        var (current, error) = await _app.Import.CommitImport(
            key, cloud ? "cloud" : "local", cloud && _storagePinned);
        if (!current)
        {
            return;
        }
        if (error is not null)
        {
            _commitError = error;
            Render();
            return;
        }
        Clear();
    }

    // The already-in-library banner: a warning line, with a jump to the
    // duplicate when its album id is known.
    private Control BuildLibraryStatusBanner(BridgeLibraryStatus status)
    {
        var banner = new Border
        {
            CornerRadius = new CornerRadius(8),
            Padding = new Thickness(10, 6),
            BorderThickness = new Thickness(1),
        };
        banner[!Border.BackgroundProperty] = new DynamicResourceExtension("BaeElevatedBrush");
        banner[!Border.BorderBrushProperty] = new DynamicResourceExtension("BaeHairlineBrush");
        var row = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 10 };
        row.Children.Add(ImportPaneUi.Cell(Loc.Chrome("import.already_in_library"), secondary: true));
        if (!string.IsNullOrEmpty(status.AlbumId))
        {
            var view = ImportPaneUi.RowButton(Loc.Chrome("import.view_in_library"));
            var albumId = status.AlbumId;
            view.Click += async (_, _) => await _dialogs.OpenAlbum(albumId);
            row.Children.Add(view);
        }
        banner.Child = row;
        return banner;
    }

    // The last import of this candidate that failed, as it survives a relaunch.
    // Shown when nothing is running for the candidate — while an import is
    // under way, its own status is the current answer.
    private Control BuildFailureBanner(BridgeImportFailure failure)
    {
        var banner = new Border
        {
            CornerRadius = new CornerRadius(8),
            Padding = new Thickness(10, 6),
            BorderThickness = new Thickness(1),
        };
        banner[!Border.BackgroundProperty] = new DynamicResourceExtension("BaeElevatedBrush");
        banner[!Border.BorderBrushProperty] = new DynamicResourceExtension("BaeHairlineBrush");
        var row = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 10 };
        row.Children.Add(ImportPaneUi.Cell(failure.Error, secondary: true));
        var retry = ImportPaneUi.RowButton(Loc.Chrome("import.row.retry"));
        retry.Click += async (_, _) => await Commit();
        row.Children.Add(retry);
        banner.Child = row;
        return banner;
    }
}
