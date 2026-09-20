using System.Collections.Generic;
using System.Linq;
using Avalonia.Controls;
using Avalonia.Headless.XUnit;
using Avalonia.Interactivity;
using Avalonia.LogicalTree;
using uniffi.bae_bridge;
using Xunit;

namespace Bae.Desktop.ViewTests;

public sealed partial class ImportMappingTableTests
{
    [AvaloniaFact]
    public void EveryCueReferenceKeepsItsOwnChoicesAndBinding()
    {
        var calls = new List<(string Sheet, string Reference, string? Audio)>();
        var table = Build(MultiFileSheetTable(), bindingOptions:
        [
            new("first.wav", "first.flac", [Offered("first.flac"), Offered("alternate.flac")]),
            new("second.wav", "second.flac", [Offered("second.flac"), Offered("replacement.flac")]),
        ], bindSheet: (sheet, reference, audio) => calls.Add((sheet, reference, audio)));

        var references = BindingMenu(table);
        Assert.Equal(new[] { "first.wav", "second.wav" }, references.Select(item => item.Header));
        var first = references[0].Items.OfType<MenuItem>().ToArray();
        var second = references[1].Items.OfType<MenuItem>().ToArray();
        Assert.Equal(new[] { "first.flac", "alternate.flac", Loc.Core("ui.import.sheet.describes_nothing") },
            first.Select(item => item.Header));
        Assert.NotNull(first[0].Icon);
        Assert.Null(first[1].Icon);
        Assert.NotNull(second[0].Icon);
        Assert.Null(second[1].Icon);
        Assert.Empty(calls);

        second[1].RaiseEvent(new RoutedEventArgs(MenuItem.ClickEvent));
        first[2].RaiseEvent(new RoutedEventArgs(MenuItem.ClickEvent));

        Assert.Equal(new (string, string, string?)[]
        {
            (SheetId, "second.wav", "replacement.flac"),
            (SheetId, "first.wav", null),
        }, calls);
    }

    [AvaloniaFact]
    public void RefusedAudioIsVisibleWithItsReasonAndCannotBeSelected()
    {
        BridgeSheetBindingOffer[] refusals =
        [
            new BridgeSheetBindingOffer.RefusedCodec("MP3"),
            new BridgeSheetBindingOffer.RefusedTiming(),
            new BridgeSheetBindingOffer.RefusedUnreadable(),
        ];
        var table = Build(MultiFileSheetTable(), bindingOptions:
        [
            new("disc.wav", "file-1.flac",
                refusals.Select((offer, index) => new BridgeSheetBindingOption($"file-{index}.flac", offer))
                    .Append(Offered("accepted.flac")).ToArray()),
        ]);
        var items = Assert.Single(BindingMenu(table)).Items.OfType<MenuItem>().ToArray();
        for (var index = 0; index < refusals.Length; index++)
        {
            Assert.False(items[index].IsEnabled);
            Assert.Contains(BridgeDisplay.RefusalLine(refusals[index])!, Assert.IsType<string>(items[index].Header));
        }
        Assert.True(items[3].IsEnabled);
        Assert.NotNull(items[1].Icon);
        Assert.Null(items[4].Icon);
    }

    [AvaloniaFact]
    public void AReferenceWithoutAudioChoicesCanStillBeCleared()
    {
        var calls = new List<(string, string, string?)>();
        var table = Build(MultiFileSheetTable(), bindingOptions:
        [
            new("missing.wav", null, []),
        ], bindSheet: (sheet, reference, audio) => calls.Add((sheet, reference, audio)));
        var reference = Assert.Single(BindingMenu(table));
        var clear = Assert.Single(reference.Items.OfType<MenuItem>());
        Assert.NotNull(clear.Icon);
        clear.RaiseEvent(new RoutedEventArgs(MenuItem.ClickEvent));
        Assert.Equal(new (string, string, string?)[] { (SheetId, "missing.wav", null) }, calls);
    }

    [AvaloniaFact]
    public void ACoreAnswerWithoutReferencesHasNoBindingChoices()
    {
        var table = Build(MultiFileSheetTable());
        var button = BindingButton(table);
        Assert.False(button.IsVisible);
        Assert.Empty(Assert.IsType<MenuFlyout>(button.Flyout).ItemsSource!);
    }

    private static BridgeMappingTable MultiFileSheetTable() => new(
        [], [],
        [new BridgeMappingFileRow.Sheet(new BridgeSheetGroup(
            SheetId, SheetId, 100, "/folder/disc.cue", new BridgeSheetBound.DescribesFiles(),
            Disc(1), [1]))],
        Reconciliation: null);

    private static BridgeSheetBindingOption Offered(string file) =>
        new(file, new BridgeSheetBindingOffer.Offered());

    private static Button BindingButton(Control table) =>
        table.GetLogicalDescendants().OfType<Button>()
            .Single(button => Equals(button.Content, Loc.Core("ui.import.sheet.choose_audio")));

    private static MenuItem[] BindingMenu(Control table) =>
        Assert.IsType<MenuFlyout>(BindingButton(table).Flyout).ItemsSource!.OfType<MenuItem>().ToArray();
}
