using System.Text.Json;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Media;
using Avalonia.Styling;
using Avalonia.Themes.Fluent;

namespace Bae.Desktop;

// Embedded from the same file BaeKit and Android bundle. Platform styles map
// these roles to native controls; there is no second copy of the palette.
internal sealed record AppearancePalette(
    Dictionary<string, Dictionary<string, Dictionary<string, string>>> Tones,
    Dictionary<string, Dictionary<string, string>> Accents,
    Dictionary<string, Dictionary<string, string>> Semantics)
{
    internal static AppearancePalette Bundled { get; } = Load();

    internal static AppearancePalette Load()
    {
        using var stream = typeof(AppearancePalette).Assembly
            .GetManifestResourceStream("Bae.AppearancePalette.json")
            ?? throw new InvalidOperationException("AppearancePalette.json is missing");
        return JsonSerializer.Deserialize<AppearancePalette>(stream,
            new JsonSerializerOptions { PropertyNameCaseInsensitive = true })
            ?? throw new InvalidDataException("AppearancePalette.json is empty");
    }

    internal Color AccentFill(AccentChoice accent) =>
        Color.Parse(Accents[accent.ToString().ToLowerInvariant()]["fill"]);

    internal void Apply(Application app, AppearancePreferences preferences)
    {
        app.Resources.ThemeDictionaries[ThemeVariant.Light] = Resources(preferences, "light");
        app.Resources.ThemeDictionaries[ThemeVariant.Dark] = Resources(preferences, "dark");
        foreach (var fluent in app.Styles.OfType<FluentTheme>())
        {
            foreach (var variant in new[] { ThemeVariant.Light, ThemeVariant.Dark })
            {
                if (!fluent.Palettes.TryGetValue(variant, out var palette))
                {
                    palette = new ColorPaletteResources();
                    fluent.Palettes[variant] = palette;
                }
                palette.Accent = AccentFill(preferences.Accent);
            }
        }
        app.RequestedThemeVariant = preferences.Mode switch
        {
            AppearanceMode.System => ThemeVariant.Default,
            AppearanceMode.Light => ThemeVariant.Light,
            AppearanceMode.Dark => ThemeVariant.Dark,
            _ => throw new ArgumentOutOfRangeException(nameof(preferences)),
        };
    }

    private ResourceDictionary Resources(AppearancePreferences preferences, string mode)
    {
        var surfaces = Tones[preferences.Tone.ToString().ToLowerInvariant()][mode];
        var accent = Accents[preferences.Accent.ToString().ToLowerInvariant()];
        var semantic = Semantics[mode];
        var resources = new ResourceDictionary();
        foreach (var (key, value) in surfaces)
        {
            resources[$"Bae{char.ToUpperInvariant(key[0])}{key[1..]}Color"] = Color.Parse(value);
        }
        foreach (var (key, value) in semantic)
        {
            resources[$"Bae{char.ToUpperInvariant(key[0])}{key[1..]}Color"] = Color.Parse(value);
        }
        var accentColor = Color.Parse(accent[mode]);
        var fill = Color.Parse(accent["fill"]);
        resources["BaeAccentColor"] = accentColor;
        resources["BaePrimaryColor"] = fill;
        resources["BaePrimaryHoverColor"] = Blend(fill, Colors.Black, 0.05);
        resources["BaePrimaryPressedColor"] = Blend(fill, Colors.Black, 0.12);
        resources["BaeOnAccentColor"] = Colors.White;
        resources["BaeSelectionTintColor"] = Color.FromArgb(36, accentColor.R, accentColor.G, accentColor.B);
        resources["SystemAccentColor"] = fill;
        return resources;
    }

    private static Color Blend(Color color, Color overlay, double amount) => Color.FromRgb(
        (byte)Math.Round(color.R * (1 - amount) + overlay.R * amount),
        (byte)Math.Round(color.G * (1 - amount) + overlay.G * amount),
        (byte)Math.Round(color.B * (1 - amount) + overlay.B * amount));
}
