using NAudio.CoreAudioApi;
using NAudio.CoreAudioApi.Interfaces;
using System;
using System.Collections.Generic;
using System.Linq;

namespace silence_.Services;

/// <summary>
/// Service for managing microphone mute state using Windows Core Audio API
/// </summary>
public class MicrophoneService : IDisposable
{
    private MMDeviceEnumerator? _deviceEnumerator;
    private MMDevice? _selectedDevice;
    private readonly List<MMDevice> _observedDevices = [];
    private readonly EndpointNotificationClient _endpointNotificationClient;
    private string? _selectedDeviceId;
    private bool _muteAllMicrophones;
    private bool? _lastKnownMuteState;

    public event Action<bool>? MuteStateChanged;
    public event Action? DevicesChanged;

    public const string ALL_MICROPHONES_ID = "__ALL_MICROPHONES__";

    public MicrophoneService()
    {
        _deviceEnumerator = new MMDeviceEnumerator();
        _endpointNotificationClient = new EndpointNotificationClient(this);
        _deviceEnumerator.RegisterEndpointNotificationCallback(_endpointNotificationClient);
        RefreshObservedDevices(raiseStateChanged: false);
    }

    /// <summary>
    /// Gets all available microphone devices
    /// </summary>
    public List<MicrophoneInfo> GetMicrophones()
    {
        var microphones = new List<MicrophoneInfo>();

        if (_deviceEnumerator == null) return microphones;

        try
        {
            var devices = _deviceEnumerator.EnumerateAudioEndPoints(DataFlow.Capture, DeviceState.Active);
            foreach (var device in devices)
            {
                microphones.Add(new MicrophoneInfo
                {
                    Id = device.ID,
                    Name = device.FriendlyName,
                    IsDefault = device.ID == GetDefaultMicrophoneId()
                });
            }
        }
        catch
        {
            // Shit happens, return empty list
        }

        return microphones;
    }

    /// <summary>
    /// Gets the default microphone device ID
    /// </summary>
    public string? GetDefaultMicrophoneId()
    {
        try
        {
            var defaultDevice = _deviceEnumerator?.GetDefaultAudioEndpoint(DataFlow.Capture, Role.Communications);
            return defaultDevice?.ID;
        }
        catch
        {
            return null;
        }
    }

    /// <summary>
    /// Selects a microphone by ID
    /// </summary>
    public void SelectMicrophone(string? deviceId)
    {
        _selectedDeviceId = deviceId;
        _muteAllMicrophones = deviceId == ALL_MICROPHONES_ID;
        UpdateSelectedDevice();
        RefreshObservedDevices(raiseStateChanged: true);
    }

    private void UpdateSelectedDevice()
    {
        _selectedDevice?.Dispose();
        _selectedDevice = null;

        if (string.IsNullOrEmpty(_selectedDeviceId) || _deviceEnumerator == null) return;

        // Skip device selection if "All microphones" is selected
        if (_muteAllMicrophones) return;

        try
        {
            _selectedDevice = _deviceEnumerator.GetDevice(_selectedDeviceId);
        }
        catch
        {
            // Device might not exist anymore, fuck it
        }
    }

    /// <summary>
    /// Gets the currently selected device, or default if none selected
    /// </summary>
    private MMDevice? GetActiveDevice()
    {
        if (_selectedDevice != null) return _selectedDevice;

        try
        {
            return _deviceEnumerator?.GetDefaultAudioEndpoint(DataFlow.Capture, Role.Communications);
        }
        catch
        {
            return null;
        }
    }

    private void RefreshObservedDevices(bool raiseStateChanged)
    {
        UnsubscribeObservedDevices();

        if (_deviceEnumerator == null)
        {
            return;
        }

        foreach (var deviceId in GetObservedDeviceIds())
        {
            try
            {
                var device = _deviceEnumerator.GetDevice(deviceId);
                device.AudioEndpointVolume.OnVolumeNotification += OnEndpointVolumeNotification;
                _observedDevices.Add(device);
            }
            catch
            {
                // Device may disappear while rebuilding subscriptions
            }
        }

        PublishMuteState(IsMuted(), raiseStateChanged);
    }

    private IEnumerable<string> GetObservedDeviceIds()
    {
        if (_deviceEnumerator == null)
        {
            yield break;
        }

        if (_muteAllMicrophones)
        {
            MMDeviceCollection devices;
            try
            {
                devices = _deviceEnumerator.EnumerateAudioEndPoints(DataFlow.Capture, DeviceState.Active);
            }
            catch
            {
                yield break;
            }

            foreach (var device in devices)
            {
                yield return device.ID;
                device.Dispose();
            }

            yield break;
        }

        var activeDeviceId = _selectedDevice?.ID;
        if (string.IsNullOrEmpty(activeDeviceId))
        {
            activeDeviceId = GetDefaultMicrophoneId();
        }

        if (!string.IsNullOrEmpty(activeDeviceId))
        {
            yield return activeDeviceId;
        }
    }

    private void UnsubscribeObservedDevices()
    {
        foreach (var device in _observedDevices)
        {
            try
            {
                device.AudioEndpointVolume.OnVolumeNotification -= OnEndpointVolumeNotification;
            }
            catch
            {
                // Ignore cleanup failures
            }

            device.Dispose();
        }

        _observedDevices.Clear();
    }

    private void OnEndpointVolumeNotification(AudioVolumeNotificationData data)
    {
        var currentMuteState = _muteAllMicrophones ? GetMuteStateAllMicrophones() : data.Muted;
        PublishMuteState(currentMuteState, raiseStateChanged: true);
    }

    private void OnCaptureDevicesChanged()
    {
        UpdateSelectedDevice();
        RefreshObservedDevices(raiseStateChanged: true);
        DevicesChanged?.Invoke();
    }

    private void PublishMuteState(bool muted, bool raiseStateChanged)
    {
        var stateChanged = _lastKnownMuteState != muted;
        _lastKnownMuteState = muted;

        if (raiseStateChanged && stateChanged)
        {
            MuteStateChanged?.Invoke(muted);
        }
    }

    /// <summary>
    /// Toggles mute state of the selected microphone
    /// </summary>
    public bool ToggleMute()
    {
        if (_muteAllMicrophones)
        {
            return ToggleMuteAllMicrophones();
        }

        var device = GetActiveDevice();
        if (device == null) return false;

        try
        {
            var newMuteState = !device.AudioEndpointVolume.Mute;
            device.AudioEndpointVolume.Mute = newMuteState;
            PublishMuteState(newMuteState, raiseStateChanged: true);
            return newMuteState;
        }
        catch
        {
            return false;
        }
        finally
        {
            if (!ReferenceEquals(device, _selectedDevice))
            {
                device.Dispose();
            }
        }
    }

    /// <summary>
    /// Toggles mute state of all microphones
    /// </summary>
    private bool ToggleMuteAllMicrophones()
    {
        if (_deviceEnumerator == null) return false;

        try
        {
            var devices = _deviceEnumerator.EnumerateAudioEndPoints(DataFlow.Capture, DeviceState.Active);
            if (devices.Count == 0) return false;

            var currentState = devices.All(device => device.AudioEndpointVolume.Mute);
            var newMuteState = !currentState;

            foreach (var device in devices)
            {
                try
                {
                    device.AudioEndpointVolume.Mute = newMuteState;
                }
                catch
                {
                    // Skip devices that fail
                }
            }

            PublishMuteState(newMuteState, raiseStateChanged: true);
            return newMuteState;
        }
        catch
        {
            return false;
        }
    }

    /// <summary>
    /// Sets mute state directly
    /// </summary>
    public bool? SetMute(bool muted)
    {
        if (_muteAllMicrophones)
        {
            return SetMuteAllMicrophones(muted);
        }

        var device = GetActiveDevice();
        if (device == null) return null;

        try
        {
            if (device.AudioEndpointVolume.Mute == muted)
            {
                return muted;
            }

            device.AudioEndpointVolume.Mute = muted;
            PublishMuteState(muted, raiseStateChanged: true);
            return muted;
        }
        catch
        {
            return null;
        }
        finally
        {
            if (!ReferenceEquals(device, _selectedDevice))
            {
                device.Dispose();
            }
        }
    }

    /// <summary>
    /// Sets mute state for all microphones
    /// </summary>
    private bool? SetMuteAllMicrophones(bool muted)
    {
        if (_deviceEnumerator == null) return null;

        try
        {
            var devices = _deviceEnumerator.EnumerateAudioEndPoints(DataFlow.Capture, DeviceState.Active);
            if (devices.Count == 0) return null;

            var currentState = devices.All(device => device.AudioEndpointVolume.Mute);
            if (currentState == muted)
            {
                return muted;
            }

            foreach (var device in devices)
            {
                try
                {
                    device.AudioEndpointVolume.Mute = muted;
                }
                catch
                {
                    // Skip devices that fail
                }
            }

            PublishMuteState(muted, raiseStateChanged: true);
            return muted;
        }
        catch
        {
            return null;
        }
    }

    /// <summary>
    /// Gets current mute state
    /// </summary>
    public bool IsMuted()
    {
        if (_muteAllMicrophones)
        {
            return GetMuteStateAllMicrophones();
        }

        var device = GetActiveDevice();
        if (device == null) return false;

        try
        {
            return device.AudioEndpointVolume.Mute;
        }
        catch
        {
            return false;
        }
        finally
        {
            if (!ReferenceEquals(device, _selectedDevice))
            {
                device.Dispose();
            }
        }
    }

    /// <summary>
    /// Gets mute state for all microphones (true only if all active microphones are muted)
    /// </summary>
    private bool GetMuteStateAllMicrophones()
    {
        if (_deviceEnumerator == null) return false;

        try
        {
            var devices = _deviceEnumerator.EnumerateAudioEndPoints(DataFlow.Capture, DeviceState.Active);
            if (devices.Count == 0) return false;

            return devices.All(device => device.AudioEndpointVolume.Mute);
        }
        catch
        {
            return false;
        }
    }

    public string? SelectedDeviceId => _selectedDeviceId;

    public void Dispose()
    {
        if (_deviceEnumerator != null)
        {
            try
            {
                _deviceEnumerator.UnregisterEndpointNotificationCallback(_endpointNotificationClient);
            }
            catch
            {
                // Ignore callback cleanup failures during shutdown
            }
        }

        UnsubscribeObservedDevices();
        _selectedDevice?.Dispose();
        _deviceEnumerator?.Dispose();
        _selectedDevice = null;
        _deviceEnumerator = null;
    }

    private sealed class EndpointNotificationClient(MicrophoneService owner) : IMMNotificationClient
    {
        public void OnDeviceStateChanged(string deviceId, DeviceState newState)
        {
            owner.OnCaptureDevicesChanged();
        }

        public void OnDeviceAdded(string pwstrDeviceId)
        {
            owner.OnCaptureDevicesChanged();
        }

        public void OnDeviceRemoved(string deviceId)
        {
            owner.OnCaptureDevicesChanged();
        }

        public void OnDefaultDeviceChanged(DataFlow flow, Role role, string defaultDeviceId)
        {
            if (flow == DataFlow.Capture)
            {
                owner.OnCaptureDevicesChanged();
            }
        }

        public void OnPropertyValueChanged(string pwstrDeviceId, PropertyKey key)
        {
        }
    }
}

public class MicrophoneInfo
{
    public required string Id { get; set; }
    public required string Name { get; set; }
    public bool IsDefault { get; set; }
}
