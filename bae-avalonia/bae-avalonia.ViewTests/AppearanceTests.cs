using System.Text.Json;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Headless.XUnit;
using Avalonia.Markup.Xaml.MarkupExtensions;
using Avalonia.Media;
using Avalonia.Styling;
using Avalonia.Threading;
using Xunit;

namespace Bae.Desktop.ViewTests;

public sealed class AppearanceTests
{
    [Fact]
    public void ARefusedWriteDoesNotPublishTheSelection()
    {
        var store = new AppearanceStore(AppearancePreferences.Default, _ => throw new IOException("refused"));
        var notifications = 0;
        store.Changed += () => notifications++;
        Assert.Throws<IOException>(() => store.Set(new(AppearanceMode.Dark, AccentChoice.Teal, SurfaceTone.Plum)));
        Assert.Equal(AppearancePreferences.Default, store.Current);
        Assert.Equal(0, notifications);
    }

    [Fact]
    public void PreferencesSurviveOpeningAnotherStoreAndRejectIncompleteFiles()
    {
        var directory = Directory.CreateTempSubdirectory("bae-appearance-");
        try
        {
            var path = Path.Combine(directory.FullName, "appearance.json");
            var selected = new AppearancePreferences(AppearanceMode.Dark, AccentChoice.Teal, SurfaceTone.Plum);
            AppearanceStore.FromFile(path).Set(selected);
            Assert.Equal(selected, AppearanceStore.FromFile(path).Current);
            Assert.False(File.Exists(path + ".tmp"));
            File.WriteAllText(path, "{}");
            Assert.Throws<JsonException>(() => AppearanceStore.FromFile(path));
        }
        finally { directory.Delete(recursive: true); }
    }

    [AvaloniaFact]
    public void EveryPaletteUpdatesAnExistingViewAndKeepsTextReadable()
    {
        var app = Application.Current!;
        var palette = AppearancePalette.Bundled;
        var surface = new Border();
        surface[!Border.BackgroundProperty] = new DynamicResourceExtension("BaeBackgroundBrush");
        var window = new Window { Content = surface };
        window.Show();
        try
        {
            foreach (var mode in new[] { AppearanceMode.Light, AppearanceMode.Dark })
            foreach (var tone in Enum.GetValues<SurfaceTone>())
            foreach (var accent in Enum.GetValues<AccentChoice>())
            {
                palette.Apply(app, new(mode, accent, tone));
                Dispatcher.UIThread.RunJobs();
                var variant = mode == AppearanceMode.Dark ? ThemeVariant.Dark : ThemeVariant.Light;
                var resources = (ResourceDictionary)app.Resources.ThemeDictionaries[variant];
                Color ColorFor(string key) => (Color)resources[key]!;
                Assert.Equal(ColorFor("BaeBackgroundColor"), Assert.IsAssignableFrom<ISolidColorBrush>(surface.Background).Color);
                foreach (var role in new[] { "Background", "Surface", "Elevated", "Field", "Tile" })
                {
                    Assert.True(Contrast(ColorFor("BaeTextPrimaryColor"), ColorFor($"Bae{role}Color")) >= 7);
                    Assert.True(Contrast(ColorFor("BaeAccentColor"), ColorFor($"Bae{role}Color")) >= 4.5, $"{mode} {tone} {accent} {role}");
                }
                foreach (var role in new[] { "Primary", "PrimaryHover", "PrimaryPressed" })
                    Assert.True(Contrast(Colors.White, ColorFor($"Bae{role}Color")) >= 4.5, $"{accent} {role}");
            }
        }
        finally
        {
            window.Close();
            palette.Apply(app, AppearancePreferences.Default);
        }
    }

    private static double Contrast(Color a, Color b)
    {
        static double Channel(byte channel)
        {
            var value = channel / 255.0;
            return value <= 0.04045 ? value / 12.92 : Math.Pow((value + 0.055) / 1.055, 2.4);
        }
        static double Luminance(Color c) => 0.2126 * Channel(c.R) + 0.7152 * Channel(c.G) + 0.0722 * Channel(c.B);
        var first = Luminance(a);
        var second = Luminance(b);
        return (Math.Max(first, second) + 0.05) / (Math.Min(first, second) + 0.05);
    }
}
