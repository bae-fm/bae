using System;
using System.Collections.Generic;
using System.Linq;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Layout;
using uniffi.bae_bridge;

namespace Bae.Desktop;

internal sealed partial class ImportMappingPane
{
    // Find online keeps both methods alive and renders only the selected one.
    private Control BuildSearchEditor()
    {
        var column = new StackPanel { Spacing = 8 };
        column.Children.Add(SearchMethodSelector());
        column.Children.Add(_findOnlineMethod == FindOnlineMethod.Automatic
            ? AutomaticHalf()
            : ManualHalf());
        return column;
    }

    private Control SearchMethodSelector()
    {
        var methods = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Spacing = 8,
            HorizontalAlignment = HorizontalAlignment.Center,
        };
        methods.Children.Add(MethodButton(
            Loc.Chrome("import.search.auto"),
            FindOnlineMethod.Automatic));
        methods.Children.Add(MethodButton(
            Loc.Chrome("import.row.search_manually"),
            FindOnlineMethod.Manual));
        return methods;
    }

    private Button MethodButton(string label, FindOnlineMethod method)
    {
        var button = ImportPaneUi.RowButton(label);
        button.IsEnabled = _findOnlineMethod != method;
        button.Click += (_, _) =>
        {
            _findOnlineMethod = method;
            Render();
        };
        return button;
    }

    private Control AutomaticHalf()
    {
        var column = new StackPanel { Spacing = 8 };
        var signals = _import.ProjectRun(_runtime).Signals;
        if (signals.Count > 0 && _key is { } signalKey)
        {
            column.Children.Add(SignalBadgeRow.Build(
                signals,
                (kind, value) => _ = _app.Import.ToggleSignalForCandidate(
                    signalKey, kind, value),
                () => _ = _app.Import.RerunIdentifyForCandidate(signalKey)));
        }
        var matches = EffectiveMatches;
        if (matches.Count > 0)
        {
            column.Children.Add(ChoiceList(matches));
        }
        else if (ShownIdentifyState is not BridgeIdentifyState.Idle
            and not BridgeIdentifyState.Triangulating)
        {
            column.Children.Add(ImportPaneUi.Cell(
                Loc.Chrome("import.search.no_automatic_matches"),
                secondary: true));
        }
        if (ShownIdentifyState is BridgeIdentifyState.Triangulating)
        {
            column.Children.Add(new Spinner { Width = 16, Height = 16 });
        }
        else if (ShownIdentifyState is BridgeIdentifyState.Idle)
        {
            var identify = ImportPaneUi.RowButton(
                Loc.Chrome("import.search.identify_automatically"));
            identify.Click += (_, _) =>
            {
                if (_key is { } key)
                {
                    _ = _import.StartInteractiveLookup(key);
                }
            };
            column.Children.Add(identify);
        }
        else if (signals.Count == 0)
        {
            var rerun = ImportPaneUi.RowButton(Loc.Chrome("import.rerun_identify"));
            rerun.Click += (_, _) =>
            {
                if (_key is { } key)
                {
                    _ = _app.Import.RerunIdentifyForCandidate(key);
                }
            };
            column.Children.Add(rerun);
        }
        return column;
    }

    // The typed search owns only its source, fields, submission, and results.
    private Control ManualHalf()
    {
        var column = new StackPanel { Spacing = 8 };

        var types = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Spacing = 8,
        };
        types.Children.Add(ManualSearchTypeButton(
            Loc.Chrome("import.search.general"),
            ManualSearchType.General));
        types.Children.Add(ManualSearchTypeButton(
            Loc.Chrome("signal.kind.catalog"),
            ManualSearchType.Catalog));
        types.Children.Add(ManualSearchTypeButton(
            Loc.Chrome("signal.kind.barcode"),
            ManualSearchType.Barcode));
        column.Children.Add(types);

        var sourceBox = ImportPaneUi.MetadataSourcePicker(_searchSource);
        sourceBox.SelectionChanged += (_, _) => _searchSource = sourceBox.SelectedIndex;
        var sourceField = new StackPanel { Spacing = 4 };
        sourceField.Children.Add(DialogUi.SectionLabel(Loc.Chrome("search.field.source")));
        sourceField.Children.Add(sourceBox);

        Control fields = _manualSearchType switch
        {
            ManualSearchType.General => GeneralSearchFields(sourceField),
            ManualSearchType.Catalog => SearchValueField(
                Loc.Chrome("signal.kind.catalog"),
                _searchCatalog,
                value => _searchCatalog = value,
                sourceField),
            ManualSearchType.Barcode => SearchValueField(
                Loc.Chrome("signal.kind.barcode"),
                _searchBarcode,
                value => _searchBarcode = value,
                sourceField),
            _ => throw new ArgumentOutOfRangeException(
                nameof(_manualSearchType), _manualSearchType, "Unknown manual search type"),
        };
        column.Children.Add(fields);

        var search = ImportPaneUi.RowButton(Loc.Chrome("action.search"));
        search.Click += async (_, _) =>
        {
            search.IsEnabled = false;
            var key = new ManualSearchKey(_manualSearchType, _searchSource);
            var query = ManualSearchQuery((BridgeMetadataSource)sourceBox.SelectedItem!);
            var (current, found) = await _app.Import.SearchReleases(query);
            search.IsEnabled = true;
            if (!current)
            {
                return;
            }
            _manualSearchResults[key] = new ManualSearchResult(
                found.Candidates ?? new List<ReleaseCandidateChoice>(),
                found.Error);
            Render();
        };

        var actions = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
        actions.Children.Add(search);
        column.Children.Add(actions);

        var resultKey = new ManualSearchKey(_manualSearchType, _searchSource);
        if (_manualSearchResults.TryGetValue(resultKey, out var result))
        {
            if (result.Error is { Length: > 0 } error)
            {
                column.Children.Add(ImportPaneUi.Cell(error, secondary: true));
            }
            else if (result.Candidates.Count > 0)
            {
                column.Children.Add(ChoiceList(result.Candidates));
            }
            else
            {
                column.Children.Add(ImportPaneUi.Cell(
                    Loc.Chrome("search.no_matches"), secondary: true));
            }
        }
        return column;
    }

    private Button ManualSearchTypeButton(
        string label,
        ManualSearchType type)
    {
        var button = ImportPaneUi.RowButton(label);
        button.IsEnabled = _manualSearchType != type;
        button.Click += (_, _) =>
        {
            _manualSearchType = type;
            Render();
        };
        return button;
    }

    private Control GeneralSearchFields(Control sourceField)
    {
        var artistField = DialogUi.Field(Loc.Chrome("import.field.artist_manual"), out var artistBox);
        artistBox.Text = _searchArtist;
        artistBox.TextChanged += (_, _) => _searchArtist = artistBox.Text ?? string.Empty;
        var albumField = DialogUi.Field(Loc.Chrome("search.field.album"), out var albumBox);
        albumBox.Text = _searchAlbum;
        albumBox.TextChanged += (_, _) => _searchAlbum = albumBox.Text ?? string.Empty;
        var fields = new Grid
        {
            ColumnDefinitions = new ColumnDefinitions("*,*,Auto"),
            ColumnSpacing = 8,
        };
        Grid.SetColumn(artistField, 0);
        Grid.SetColumn(albumField, 1);
        Grid.SetColumn(sourceField, 2);
        fields.Children.Add(artistField);
        fields.Children.Add(albumField);
        fields.Children.Add(sourceField);
        return fields;
    }

    private static Control SearchValueField(
        string label,
        string value,
        Action<string> onChange,
        Control sourceField)
    {
        var valueField = DialogUi.Field(label, out var valueBox);
        valueBox.Text = value;
        valueBox.TextChanged += (_, _) => onChange(valueBox.Text ?? string.Empty);
        var fields = new Grid
        {
            ColumnDefinitions = new ColumnDefinitions("*,Auto"),
            ColumnSpacing = 8,
        };
        Grid.SetColumn(valueField, 0);
        Grid.SetColumn(sourceField, 1);
        fields.Children.Add(valueField);
        fields.Children.Add(sourceField);
        return fields;
    }

    private BridgeSearchQuery ManualSearchQuery(BridgeMetadataSource source) =>
        _manualSearchType switch
        {
            ManualSearchType.General => new BridgeSearchQuery.General(
                _searchArtist,
                _searchAlbum,
                source),
            ManualSearchType.Catalog => new BridgeSearchQuery.CatalogNumber(
                _searchCatalog,
                source),
            ManualSearchType.Barcode => new BridgeSearchQuery.Barcode(
                _searchBarcode,
                source),
            _ => throw new ArgumentOutOfRangeException(
                nameof(_manualSearchType), _manualSearchType, "Unknown manual search type"),
        };

    // The pressings on offer, whichever half produced them. Picking one
    // applies that external release, and the editor closes only after the
    // candidate subscription delivers that provenance too.
    private Control ChoiceList(List<ReleaseCandidateChoice> choices)
    {
        var results = new ListBox { SelectionMode = SelectionMode.Single, MaxHeight = 190 };
        results.ItemsSource = choices.Select(ChoiceRow).ToList();
        results.IsEnabled = _applyingProvenance is null;
        results.SelectionChanged += async (_, _) =>
        {
            if (results.SelectedIndex < 0 || results.SelectedIndex >= choices.Count)
            {
                return;
            }
            var chosen = choices[results.SelectedIndex];
            await ApplyMetadata(
                new BridgeMetadataProvenance.ExternalRelease(
                    chosen.Source,
                    chosen.ReleaseId));
        };
        return results;
    }

    private Control ChoiceRow(ReleaseCandidateChoice choice)
    {
        var row = new Grid
        {
            ColumnDefinitions = new ColumnDefinitions("*,Auto"),
            ColumnSpacing = 8,
            Margin = new Thickness(4, 6),
        };
        row.Children.Add(ImportPaneUi.Cell(choice.Summary));
        var progress = new Spinner
        {
            Width = 14,
            Height = 14,
            IsVisible = _applyingProvenance is BridgeMetadataProvenance.ExternalRelease applying
                && applying.Source == choice.Source
                && applying.ReleaseId == choice.ReleaseId,
        };
        Grid.SetColumn(progress, 1);
        row.Children.Add(progress);
        return row;
    }
}
