using System;
using System.Collections.Generic;
using Avalonia.Headless.XUnit;
using Avalonia.Threading;
using Bae.Desktop;
using uniffi.bae_bridge;
using Xunit;

namespace Bae.Desktop.ViewTests;

/// <summary>
/// The now-playing bar's cast control is present only while casting is turned
/// on, and follows the setting live. Core is what actually gates discovery and
/// connection; this covers the presentation half.
/// </summary>
public sealed class CastButtonTests
{
    [AvaloniaFact]
    public void TheControlIsAbsentWhileCastingIsOff()
    {
        var (button, _, _) = Build(castEnabled: false);

        Assert.False(button.IsVisible);
    }

    [AvaloniaFact]
    public void TheControlIsPresentWhileCastingIsOn()
    {
        var (button, _, _) = Build(castEnabled: true);

        Assert.True(button.IsVisible);
    }

    [AvaloniaFact]
    public void TheControlFollowsTheSettingWithoutBeingRebuilt()
    {
        var (button, settings, castEnabled) = Build(castEnabled: true);

        castEnabled.Value = false;
        settings.Reload();
        Assert.False(button.IsVisible);

        castEnabled.Value = true;
        settings.Reload();
        Assert.True(button.IsVisible);
    }

    /// <summary>A cast button over stub services, plus the settings mirror it
    /// reads and a cell for flipping the persisted setting between reloads.</summary>
    private static (CastButton Button, SettingsStore Settings, Cell CastEnabled) Build(bool castEnabled)
    {
        // Constructing controls is only legal on the headless session's
        // dispatcher thread, which is what [AvaloniaFact] puts a test body on.
        Dispatcher.UIThread.VerifyAccess();

        var cell = new Cell { Value = castEnabled };
        var settings = new SettingsStore(new SettingsService
        {
            GetSettings = () => (true, new Settings { CastEnabled = cell.Value }),
        });
        settings.Reload();
        var cast = new CastStore(new CastService
        {
            GetCastDevices = () => (true, Array.Empty<BridgeCastDevice>()),
            StartDiscovery = () => true,
            StopDiscovery = () => true,
        });
        return (new CastButton(cast, settings), settings, cell);
    }

    private sealed class Cell
    {
        public bool Value { get; set; }
    }
}
