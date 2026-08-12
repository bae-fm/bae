using System;
using System.Collections.Generic;
using uniffi.bae_bridge;

namespace Bae.Desktop;

// Cast state for the now-playing bar's cast control: the current cast status
// (which device, if any) and the discovered device list. The retained playback
// values drive status, including receiver-side ends; the cast-device value stream
// drives the device list while the picker is open. Discovery runs only while the
// picker is open.
internal sealed class CastStore
{
    private readonly CastService _cast;

    // The devices found on the network, refreshed by the castDevices projection.
    public IReadOnlyList<BridgeCastDevice> Devices { get; private set; } = Array.Empty<BridgeCastDevice>();

    // The device name while casting, or null on local output.
    public string? CastingDeviceName { get; private set; }

    public bool IsCasting => CastingDeviceName is not null;

    // Fires when the cast status changes (active/inactive, device name).
    public event Action? StatusChanged;

    // Fires when the discovered device list changes.
    public event Action? DevicesChanged;

    public CastStore(CastService cast)
    {
        _cast = cast;
    }

    // Apply the retained playback value: the device name while casting, null back
    // on local output.
    public void ApplyStatus(string? deviceName)
    {
        CastingDeviceName = deviceName;
        StatusChanged?.Invoke();
    }

    public void ApplyDevices(IReadOnlyList<BridgeCastDevice> devices)
    {
        Devices = devices;
        DevicesChanged?.Invoke();
    }

    // Begin/stop browsing (the picker opened/closed).
    public void StartDiscovery() => _cast.StartDiscovery();

    public void StopDiscovery() => _cast.StopDiscovery();

    // Switch playback to a device. Returns a localized error string on failure,
    // or null on success (or when there is no open library).
    public string? CastTo(string deviceId)
    {
        var (current, error) = _cast.CastTo(deviceId);
        return current ? error : null;
    }

    // Stop casting and return playback to local output.
    public void StopCasting() => _cast.StopCasting();

    // Turn the whole feature on or off. Returns a localized error string on
    // failure, or null on success (or when there is no open library).
    public string? SetEnabled(bool enabled)
    {
        var (current, error) = _cast.SetEnabled(enabled);
        return current ? error : null;
    }
}
