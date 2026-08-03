using Avalonia;
using Avalonia.Controls;
using Avalonia.Layout;
using Avalonia.Markup.Xaml.MarkupExtensions;
using Avalonia.Media;

namespace Bae.Desktop;

// The now-playing bar's cast control: a cast glyph opening a device-picker flyout.
// Discovery runs only while the flyout is open (browsing the network is not
// always-on). While casting the glyph tints to the accent over a soft accent fill
// and the flyout carries a "Casting to …" header plus a disconnect action; the
// discovered devices follow, or an empty-state line when none are found. The
// device list refreshes on the castDevices projection and the active device on the
// castStatusChanged event, both mirrored into CastStore.
//
// The control is present only while casting is turned on. That is presentation
// only: core is what refuses to browse or connect when the setting is off.
internal sealed class CastButton : Button
{
    private readonly CastStore _cast;
    private readonly SettingsStore _settings;
    private readonly PathIcon _glyph;
    private readonly MenuFlyout _flyout;
    private bool _open;

    public CastButton(CastStore cast, SettingsStore settings)
    {
        _cast = cast;
        _settings = settings;

        Width = 30;
        Height = 30;
        Padding = new Thickness(0);
        Background = Brushes.Transparent;
        BorderThickness = new Thickness(0);
        HorizontalContentAlignment = HorizontalAlignment.Center;
        VerticalContentAlignment = VerticalAlignment.Center;
        _glyph = Icons.Glyph(Icons.Cast, 16, "BaeTextSecondaryBrush");
        Content = _glyph;

        _flyout = new MenuFlyout { Placement = PlacementMode.Top };
        Flyout = _flyout;
        _flyout.Opening += (_, _) =>
        {
            _open = true;
            _cast.StartDiscovery();
            Populate();
        };
        _flyout.Closed += (_, _) =>
        {
            _open = false;
            _cast.StopDiscovery();
        };

        _cast.StatusChanged += OnStatusChanged;
        _cast.DevicesChanged += OnDevicesChanged;
        _settings.Changed += ApplyEnabled;
        ApplyEnabled();
        Render();
    }

    // Show the control only while the setting is on. No settings snapshot yet
    // means no library is open, so there is nothing to cast either way.
    private void ApplyEnabled()
    {
        IsVisible = _settings.Current is { CastEnabled: true };
        if (!IsVisible && _open)
        {
            _flyout.Hide();
        }
    }

    private void OnStatusChanged()
    {
        Render();
        if (_open)
        {
            Populate();
        }
    }

    private void OnDevicesChanged()
    {
        if (_open)
        {
            Populate();
        }
    }

    // The glyph tints to the accent over a soft accent fill while casting, neutral
    // otherwise; the accessible name reads the active device or "Cast".
    private void Render()
    {
        var casting = _cast.IsCasting;
        _glyph[!PathIcon.ForegroundProperty] = new DynamicResourceExtension(
            casting ? "BaeAccentBrush" : "BaeTextSecondaryBrush");
        if (casting)
        {
            this[!BackgroundProperty] = new DynamicResourceExtension("BaeSelectionTintBrush");
        }
        else
        {
            Background = Brushes.Transparent;
        }
        var name = casting
            ? Loc.Chrome("nowplaying.casting_to", "device", _cast.CastingDeviceName ?? string.Empty)
            : Loc.Chrome("nowplaying.cast");
        Avalonia.Automation.AutomationProperties.SetName(this, name);
        ToolTip.SetTip(this, name);
    }

    // Rebuild the device picker: the active-casting header and disconnect action
    // (when casting), then the discovered devices, or an empty-state line.
    private void Populate()
    {
        _flyout.Items.Clear();

        if (_cast.CastingDeviceName is { } castingName)
        {
            _flyout.Items.Add(new MenuItem
            {
                Header = Loc.Chrome("nowplaying.casting_to", "device", castingName),
                IsEnabled = false,
            });
            var disconnect = new MenuItem { Header = Loc.Chrome("nowplaying.cast_disconnect") };
            disconnect.Click += (_, _) => _cast.StopCasting();
            _flyout.Items.Add(disconnect);
            _flyout.Items.Add(new Separator());
        }

        if (_cast.Devices.Count == 0)
        {
            _flyout.Items.Add(new MenuItem
            {
                Header = Loc.Chrome("nowplaying.cast_no_devices"),
                IsEnabled = false,
            });
            return;
        }

        foreach (var device in _cast.Devices)
        {
            var deviceId = device.Id;
            var item = new MenuItem { Header = device.Name };
            item.Click += (_, _) =>
            {
                var error = _cast.CastTo(deviceId);
                if (error is not null)
                {
                    BaeDiagnostics.Logger.Warning($"Cast to {deviceId} failed: {error}");
                }
            };
            _flyout.Items.Add(item);
        }
    }
}
