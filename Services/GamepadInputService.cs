using System;
using System.Collections.Generic;
using System.Linq;
using System.Runtime.InteropServices;
using System.Threading;

namespace silence_.Services;

public partial class GamepadInputService(
    Func<IReadOnlyList<int>>? recordingChordKeysProvider = null,
    Func<IReadOnlyList<int>>? pressedChordKeysProvider = null) : IDisposable
{
    private const int PollingIntervalMs = 33;
    private const double RecordingHoldDurationSeconds = 1.0;
    private const int MaxXInputUsers = 4;
    private const int XInputSuccess = 0;
    private const byte TriggerPressThresholdByte = 191;
    private const ushort XInputGamepadDPadUp = 0x0001;
    private const ushort XInputGamepadDPadDown = 0x0002;
    private const ushort XInputGamepadDPadLeft = 0x0004;
    private const ushort XInputGamepadDPadRight = 0x0008;
    private const ushort XInputGamepadStart = 0x0010;
    private const ushort XInputGamepadBack = 0x0020;
    private const ushort XInputGamepadLeftThumb = 0x0040;
    private const ushort XInputGamepadRightThumb = 0x0080;
    private const ushort XInputGamepadLeftShoulder = 0x0100;
    private const ushort XInputGamepadRightShoulder = 0x0200;
    private const ushort XInputGamepadA = 0x1000;
    private const ushort XInputGamepadB = 0x2000;
    private const ushort XInputGamepadX = 0x4000;
    private const ushort XInputGamepadY = 0x8000;

    private readonly object _syncLock = new();
    private readonly List<HotkeyBindingSettings> _hotkeys = [];
    private readonly List<HoldHotkeyBindingSettings> _holdHotkeys = [];
    private readonly Dictionary<int, ulong> _previousPressedMasks = [];
    private readonly Func<IReadOnlyList<int>> _recordingChordKeysProvider = recordingChordKeysProvider ?? (() => []);
    private readonly Func<IReadOnlyList<int>> _pressedChordKeysProvider = pressedChordKeysProvider ?? (() => []);

    private Timer? _pollTimer;
    private ulong _activeHoldButtonsMask;
    private int? _activeHoldSourceUserIndex;
    private List<int> _activeHoldChordKeys = [];
    private List<int> _previousPressedChordKeys = [];
    private int? _recordingSourceUserIndex;
    private ulong _recordingButtonsMask;
    private List<int> _recordingChordKeys = [];
    private DateTime _recordingButtonsStartTime;
    private ulong _lastPreviewMask;
    private string _lastPreviewChordSignature = string.Empty;

    public event Action<string>? HotkeyPressed;
    public event Action? HoldHotkeyPressed;
    public event Action? HoldHotkeyReleased;
    public event Action<ulong, IReadOnlyList<int>>? ButtonsCaptured;
    public event Action<ulong, IReadOnlyList<int>>? RecordingButtonsChanged;
    public event Action<double>? ButtonHoldProgress;

    public bool IsRecording { get; set; }

    public void StartMonitoring(
        IEnumerable<HotkeyBindingSettings>? hotkeys = null,
        IEnumerable<HoldHotkeyBindingSettings>? holdHotkeys = null)
    {
        lock (_syncLock)
        {
            UpdateHotkeysLocked(hotkeys);
            UpdateHoldHotkeysLocked(holdHotkeys);
            ResetActiveHoldLocked();
            ResetRecordingStateLocked();
            PrimeKnownGamepadsLocked();
            _pollTimer?.Dispose();
            _pollTimer = new Timer(PollGamepads, null, PollingIntervalMs, PollingIntervalMs);
        }
    }

    public void StopMonitoring()
    {
        lock (_syncLock)
        {
            _pollTimer?.Dispose();
            _pollTimer = null;
            _previousPressedMasks.Clear();
            _previousPressedChordKeys = [];
            ResetActiveHoldLocked();
            ResetRecordingStateLocked();
        }
    }

    public void UpdateHotkeys(IEnumerable<HotkeyBindingSettings>? hotkeys)
    {
        lock (_syncLock)
        {
            UpdateHotkeysLocked(hotkeys);
        }
    }

    public void UpdateHoldHotkeys(IEnumerable<HoldHotkeyBindingSettings>? holdHotkeys)
    {
        lock (_syncLock)
        {
            UpdateHoldHotkeysLocked(holdHotkeys);
            ResetActiveHoldLocked();
        }
    }

    public void ResetRecordingState()
    {
        lock (_syncLock)
        {
            ResetRecordingStateLocked();
        }

        RecordingButtonsChanged?.Invoke(0, []);
        ButtonHoldProgress?.Invoke(0);
    }

    private void UpdateHotkeysLocked(IEnumerable<HotkeyBindingSettings>? hotkeys)
    {
        _hotkeys.Clear();

        if (hotkeys == null)
        {
            return;
        }

        foreach (var hotkey in hotkeys)
        {
            if (hotkey == null)
            {
                continue;
            }

            var requiredMask = GetRequiredMask(hotkey);
            List<int> chordKeyCodes = hotkey.ChordKeyCodes ?? [];
            if (
                hotkey.DeviceKind != InputDeviceKind.Gamepad ||
                (requiredMask == 0 && chordKeyCodes.Count == 0))
            {
                continue;
            }

            _hotkeys.Add(new HotkeyBindingSettings
            {
                Id = hotkey.Id,
                Action = hotkey.Action,
                DeviceKind = hotkey.DeviceKind,
                GamepadButton = hotkey.GamepadButton,
                GamepadButtonsMask = requiredMask,
                ChordKeyCodes = [.. chordKeyCodes]
            });
        }
    }

    private void UpdateHoldHotkeysLocked(IEnumerable<HoldHotkeyBindingSettings>? holdHotkeys)
    {
        _holdHotkeys.Clear();

        if (holdHotkeys == null)
        {
            return;
        }

        foreach (var hotkey in holdHotkeys)
        {
            if (hotkey == null)
            {
                continue;
            }

            var requiredMask = GetRequiredMask(hotkey);
            List<int> chordKeyCodes = hotkey.ChordKeyCodes ?? [];
            if (
                hotkey.DeviceKind != InputDeviceKind.Gamepad ||
                (requiredMask == 0 && chordKeyCodes.Count == 0))
            {
                continue;
            }

            _holdHotkeys.Add(new HoldHotkeyBindingSettings
            {
                Id = hotkey.Id,
                DeviceKind = hotkey.DeviceKind,
                GamepadButton = hotkey.GamepadButton,
                GamepadButtonsMask = requiredMask,
                ChordKeyCodes = [.. chordKeyCodes]
            });
        }
    }

    private void PollGamepads(object? state)
    {
        if (!Monitor.TryEnter(_syncLock))
        {
            return;
        }

        List<Action> pendingActions;
        try
        {
            pendingActions = PollGamepadsLocked();
        }
        finally
        {
            Monitor.Exit(_syncLock);
        }

        foreach (var action in pendingActions)
        {
            action();
        }
    }

    private List<Action> PollGamepadsLocked()
    {
        List<Action> pendingActions = [];
        Dictionary<int, ulong> currentMasks = new(MaxXInputUsers);
        Dictionary<int, ulong> previousMasks = new(MaxXInputUsers);
        List<int> currentChordKeys = NormalizeChordKeys(_pressedChordKeysProvider());
        List<int> previousChordKeys = [.. _previousPressedChordKeys];
        List<int> recordingChordKeys = NormalizeChordKeys(_recordingChordKeysProvider());

        foreach (var userIndex in EnumerateConnectedControllers())
        {
            if (!TryGetControllerState(userIndex, out var state))
            {
                continue;
            }

            currentMasks[userIndex] = GetPressedMask(state.Gamepad);
            _previousPressedMasks.TryGetValue(userIndex, out var previousMask);
            previousMasks[userIndex] = previousMask;
        }

        RemoveDisconnectedGamepadsLocked(currentMasks.Keys, pendingActions);

        if (!IsRecording)
        {
            HandleChordOnlyHoldStateLocked(previousChordKeys, currentChordKeys, pendingActions);
        }

        foreach (var pair in currentMasks)
        {
            var previousMask = previousMasks.TryGetValue(pair.Key, out var storedPreviousMask)
                ? storedPreviousMask
                : 0;

            if (!IsRecording)
            {
                HandleHoldStateLocked(pair.Key, previousMask, pair.Value, previousChordKeys, currentChordKeys, pendingActions);
            }

            _previousPressedMasks[pair.Key] = pair.Value;
        }

        if (IsRecording)
        {
            UpdateRecordingStateLocked(currentMasks, recordingChordKeys, pendingActions);
        }
        else
        {
            TriggerHotkeysLocked(currentMasks, previousMasks, previousChordKeys, currentChordKeys, pendingActions);
            TriggerChordOnlyHotkeysLocked(previousChordKeys, currentChordKeys, pendingActions);
        }

        _previousPressedChordKeys = [.. currentChordKeys];

        return pendingActions;
    }

    private void HandleHoldStateLocked(
        int userIndex,
        ulong previousMask,
        ulong currentMask,
        IReadOnlyList<int> previousChordKeys,
        IReadOnlyList<int> currentChordKeys,
        List<Action> pendingActions)
    {
        if (_activeHoldSourceUserIndex == userIndex && (_activeHoldButtonsMask != 0 || _activeHoldChordKeys.Count > 0))
        {
            if (currentMask != _activeHoldButtonsMask ||
                !ChordKeysEqual(currentChordKeys, _activeHoldChordKeys))
            {
                ResetActiveHoldLocked();
                pendingActions.Add(() => HoldHotkeyReleased?.Invoke());
            }

            return;
        }

        if (_activeHoldSourceUserIndex != null)
        {
            return;
        }

        var activatedHold = _holdHotkeys
            .Select(hotkey => new { Hotkey = hotkey, Mask = GetRequiredMask(hotkey) })
            .Where(candidate => candidate.Mask != 0 &&
                                currentMask == candidate.Mask &&
                                (previousMask != candidate.Mask ||
                                 !ChordKeysEqual(previousChordKeys, candidate.Hotkey.ChordKeyCodes)) &&
                                ChordKeysEqual(currentChordKeys, candidate.Hotkey.ChordKeyCodes))
            .OrderByDescending(candidate => CountBits(candidate.Mask))
            .FirstOrDefault();

        if (activatedHold == null)
        {
            return;
        }

        _activeHoldSourceUserIndex = userIndex;
        _activeHoldButtonsMask = activatedHold.Mask;
        _activeHoldChordKeys = [.. activatedHold.Hotkey.ChordKeyCodes];
        pendingActions.Add(() => HoldHotkeyPressed?.Invoke());
    }

    private void HandleChordOnlyHoldStateLocked(
        IReadOnlyList<int> previousChordKeys,
        IReadOnlyList<int> currentChordKeys,
        List<Action> pendingActions)
    {
        if (_activeHoldSourceUserIndex == null && _activeHoldButtonsMask == 0 && _activeHoldChordKeys.Count > 0)
        {
            if (!ChordKeysEqual(currentChordKeys, _activeHoldChordKeys))
            {
                ResetActiveHoldLocked();
                pendingActions.Add(() => HoldHotkeyReleased?.Invoke());
            }

            return;
        }

        if (_activeHoldSourceUserIndex != null)
        {
            return;
        }

        var activatedHold = _holdHotkeys
            .Where(hotkey => GetRequiredMask(hotkey) == 0 && hotkey.ChordKeyCodes.Count > 0)
            .Where(hotkey =>
                !ChordKeysEqual(previousChordKeys, hotkey.ChordKeyCodes) &&
                ChordKeysEqual(currentChordKeys, hotkey.ChordKeyCodes))
            .OrderByDescending(hotkey => hotkey.ChordKeyCodes.Count)
            .FirstOrDefault();

        if (activatedHold == null)
        {
            return;
        }

        _activeHoldSourceUserIndex = null;
        _activeHoldButtonsMask = 0;
        _activeHoldChordKeys = [.. activatedHold.ChordKeyCodes];
        pendingActions.Add(() => HoldHotkeyPressed?.Invoke());
    }

    private void TriggerHotkeysLocked(
        Dictionary<int, ulong> currentMasks,
        Dictionary<int, ulong> previousMasks,
        IReadOnlyList<int> previousChordKeys,
        IReadOnlyList<int> currentChordKeys,
        List<Action> pendingActions)
    {
        foreach (var pair in currentMasks)
        {
            var previousMask = previousMasks.TryGetValue(pair.Key, out var storedPreviousMask)
                ? storedPreviousMask
                : 0;
            var currentMask = pair.Value;

            if (_activeHoldSourceUserIndex == pair.Key && currentMask == _activeHoldButtonsMask)
            {
                continue;
            }

            var activatedHotkey = _hotkeys
                .Select(hotkey => new { Hotkey = hotkey, Mask = GetRequiredMask(hotkey) })
                .Where(candidate => candidate.Mask != 0 &&
                                    currentMask == candidate.Mask &&
                                    (previousMask != candidate.Mask ||
                                     !ChordKeysEqual(previousChordKeys, candidate.Hotkey.ChordKeyCodes)) &&
                                    ChordKeysEqual(currentChordKeys, candidate.Hotkey.ChordKeyCodes) &&
                                    !IsConflictingWithHoldHotkey(candidate.Mask, currentChordKeys))
                .OrderByDescending(candidate => CountBits(candidate.Mask))
                .FirstOrDefault();

            if (activatedHotkey == null)
            {
                continue;
            }

            var action = activatedHotkey.Hotkey.Action;
            pendingActions.Add(() => HotkeyPressed?.Invoke(action));
        }
    }

    private void TriggerChordOnlyHotkeysLocked(
        IReadOnlyList<int> previousChordKeys,
        IReadOnlyList<int> currentChordKeys,
        List<Action> pendingActions)
    {
        var activatedHotkey = _hotkeys
            .Where(hotkey => GetRequiredMask(hotkey) == 0 && hotkey.ChordKeyCodes.Count > 0)
            .Where(hotkey =>
                !ChordKeysEqual(previousChordKeys, hotkey.ChordKeyCodes) &&
                ChordKeysEqual(currentChordKeys, hotkey.ChordKeyCodes) &&
                !IsConflictingWithHoldHotkey(0, currentChordKeys))
            .OrderByDescending(hotkey => hotkey.ChordKeyCodes.Count)
            .FirstOrDefault();

        if (activatedHotkey == null)
        {
            return;
        }

        pendingActions.Add(() => HotkeyPressed?.Invoke(activatedHotkey.Action));
    }

    private void UpdateRecordingStateLocked(
        Dictionary<int, ulong> currentMasks,
        List<int> recordingChordKeys,
        List<Action> pendingActions)
    {
        var currentCombinedMask = currentMasks.Values.Aggregate(0UL, static (current, next) => current | next);

        if (_recordingButtonsMask != 0 || _recordingChordKeys.Count > 0)
        {
            if (currentCombinedMask == 0 && recordingChordKeys.Count == 0)
            {
                ResetRecordingStateLocked();
                QueueRecordingUiUpdate(pendingActions, 0, [], 0);
                return;
            }

            if (currentCombinedMask != _recordingButtonsMask ||
                !ChordKeysEqual(recordingChordKeys, _recordingChordKeys))
            {
                _recordingButtonsMask = currentCombinedMask;
                _recordingChordKeys = [.. recordingChordKeys];
                _recordingButtonsStartTime = DateTime.UtcNow;
                QueueRecordingUiUpdate(pendingActions, currentCombinedMask, _recordingChordKeys, 0);
                return;
            }

            var elapsed = (DateTime.UtcNow - _recordingButtonsStartTime).TotalSeconds;
            var progress = Math.Min(elapsed / RecordingHoldDurationSeconds, 1.0);
            QueueRecordingUiUpdate(pendingActions, currentCombinedMask, _recordingChordKeys, progress);

            if (progress < 1.0)
            {
                return;
            }

            var capturedMask = _recordingButtonsMask;
            List<int> capturedChordKeys = [.. _recordingChordKeys];
            IsRecording = false;
            ResetRecordingStateLocked();
            QueueRecordingUiUpdate(pendingActions, 0, [], 0);
            pendingActions.Add(() => ButtonsCaptured?.Invoke(capturedMask, capturedChordKeys));
            return;
        }

        if (currentCombinedMask == 0 && recordingChordKeys.Count == 0)
        {
            QueueRecordingUiUpdate(pendingActions, 0, [], 0);
            return;
        }

        _recordingSourceUserIndex = currentMasks.Keys.FirstOrDefault(-1);
        _recordingButtonsMask = currentCombinedMask;
        _recordingChordKeys = [.. recordingChordKeys];
        _recordingButtonsStartTime = DateTime.UtcNow;
        QueueRecordingUiUpdate(pendingActions, _recordingButtonsMask, _recordingChordKeys, 0);
    }

    private void QueueRecordingUiUpdate(
        List<Action> pendingActions,
        ulong previewMask,
        IReadOnlyList<int> previewChordKeys,
        double progress)
    {
        var chordSignature = GetChordKeySignature(previewChordKeys);
        if (_lastPreviewMask != previewMask || !string.Equals(_lastPreviewChordSignature, chordSignature, StringComparison.Ordinal))
        {
            _lastPreviewMask = previewMask;
            _lastPreviewChordSignature = chordSignature;
            List<int> chordKeysSnapshot = [.. previewChordKeys];
            pendingActions.Add(() => RecordingButtonsChanged?.Invoke(previewMask, chordKeysSnapshot));
        }

        pendingActions.Add(() => ButtonHoldProgress?.Invoke(progress));
    }

    private void RemoveDisconnectedGamepadsLocked(IEnumerable<int> connectedGamepads, List<Action> pendingActions)
    {
        var connectedSet = connectedGamepads.ToHashSet();
        var disconnectedGamepads = _previousPressedMasks.Keys
            .Where(existing => !connectedSet.Contains(existing))
            .ToArray();

        foreach (var userIndex in disconnectedGamepads)
        {
            _previousPressedMasks.Remove(userIndex);

            if (_recordingSourceUserIndex == userIndex)
            {
                ResetRecordingStateLocked();
                QueueRecordingUiUpdate(pendingActions, 0, [], 0);
            }

            if (_activeHoldSourceUserIndex == userIndex)
            {
                ResetActiveHoldLocked();
                pendingActions.Add(() => HoldHotkeyReleased?.Invoke());
            }
        }
    }

    private void PrimeKnownGamepadsLocked()
    {
        _previousPressedMasks.Clear();
        _previousPressedChordKeys = [];

        foreach (var userIndex in EnumerateConnectedControllers())
        {
            if (TryGetControllerState(userIndex, out var state))
            {
                _previousPressedMasks[userIndex] = GetPressedMask(state.Gamepad);
            }
        }
    }

    private void ResetActiveHoldLocked()
    {
        _activeHoldButtonsMask = 0;
        _activeHoldSourceUserIndex = null;
        _activeHoldChordKeys = [];
    }

    private void ResetRecordingStateLocked()
    {
        _recordingSourceUserIndex = null;
        _recordingButtonsMask = 0;
        _recordingChordKeys = [];
        _recordingButtonsStartTime = default;
        _lastPreviewMask = 0;
        _lastPreviewChordSignature = string.Empty;
    }

    private static IEnumerable<int> EnumerateConnectedControllers()
    {
        for (int userIndex = 0; userIndex < MaxXInputUsers; userIndex++)
        {
            if (TryGetControllerState(userIndex, out _))
            {
                yield return userIndex;
            }
        }
    }

    private static bool TryGetControllerState(int userIndex, out XInputState state)
    {
        return XInputGetState((uint)userIndex, out state) == XInputSuccess;
    }

    private static ulong GetPressedMask(XInputGamepad gamepad)
    {
        ulong mask = 0;

        if ((gamepad.wButtons & XInputGamepadA) != 0) mask |= InputBindingDisplay.ToMask(GamepadButtonId.A);
        if ((gamepad.wButtons & XInputGamepadB) != 0) mask |= InputBindingDisplay.ToMask(GamepadButtonId.B);
        if ((gamepad.wButtons & XInputGamepadX) != 0) mask |= InputBindingDisplay.ToMask(GamepadButtonId.X);
        if ((gamepad.wButtons & XInputGamepadY) != 0) mask |= InputBindingDisplay.ToMask(GamepadButtonId.Y);
        if ((gamepad.wButtons & XInputGamepadLeftShoulder) != 0) mask |= InputBindingDisplay.ToMask(GamepadButtonId.LeftShoulder);
        if ((gamepad.wButtons & XInputGamepadRightShoulder) != 0) mask |= InputBindingDisplay.ToMask(GamepadButtonId.RightShoulder);
        if (gamepad.bLeftTrigger >= TriggerPressThresholdByte) mask |= InputBindingDisplay.ToMask(GamepadButtonId.LeftTrigger);
        if (gamepad.bRightTrigger >= TriggerPressThresholdByte) mask |= InputBindingDisplay.ToMask(GamepadButtonId.RightTrigger);
        if ((gamepad.wButtons & XInputGamepadBack) != 0) mask |= InputBindingDisplay.ToMask(GamepadButtonId.View);
        if ((gamepad.wButtons & XInputGamepadStart) != 0) mask |= InputBindingDisplay.ToMask(GamepadButtonId.Menu);
        if ((gamepad.wButtons & XInputGamepadDPadUp) != 0) mask |= InputBindingDisplay.ToMask(GamepadButtonId.DPadUp);
        if ((gamepad.wButtons & XInputGamepadDPadDown) != 0) mask |= InputBindingDisplay.ToMask(GamepadButtonId.DPadDown);
        if ((gamepad.wButtons & XInputGamepadDPadLeft) != 0) mask |= InputBindingDisplay.ToMask(GamepadButtonId.DPadLeft);
        if ((gamepad.wButtons & XInputGamepadDPadRight) != 0) mask |= InputBindingDisplay.ToMask(GamepadButtonId.DPadRight);
        if ((gamepad.wButtons & XInputGamepadLeftThumb) != 0) mask |= InputBindingDisplay.ToMask(GamepadButtonId.LeftThumbstick);
        if ((gamepad.wButtons & XInputGamepadRightThumb) != 0) mask |= InputBindingDisplay.ToMask(GamepadButtonId.RightThumbstick);
        return mask;
    }

    private static ulong GetRequiredMask(HotkeyBindingSettings hotkey)
    {
        return hotkey.GamepadButtonsMask != 0
            ? hotkey.GamepadButtonsMask
            : InputBindingDisplay.ToMask(hotkey.GamepadButton);
    }

    private static ulong GetRequiredMask(HoldHotkeyBindingSettings hotkey)
    {
        return hotkey.GamepadButtonsMask != 0
            ? hotkey.GamepadButtonsMask
            : InputBindingDisplay.ToMask(hotkey.GamepadButton);
    }

    private bool IsConflictingWithHoldHotkey(ulong mask, IReadOnlyList<int> chordKeys)
    {
        return _holdHotkeys.Any(hotkey =>
            GetRequiredMask(hotkey) == mask &&
            ChordKeysEqual(chordKeys, hotkey.ChordKeyCodes));
    }

    private static List<int> NormalizeChordKeys(IReadOnlyList<int> chordKeys)
    {
        List<int> normalizedChordKeys =
        [
            .. chordKeys
            .Where(VirtualKeys.IsChordableGenericKey)
            .Distinct()
            .OrderBy(code => code)
        ];

        return normalizedChordKeys;
    }

    private static bool ChordKeysEqual(IReadOnlyList<int> currentChordKeys, List<int>? requiredChordKeys)
    {
        requiredChordKeys ??= [];
        if (currentChordKeys.Count != requiredChordKeys.Count)
        {
            return false;
        }

        for (int index = 0; index < currentChordKeys.Count; index++)
        {
            if (currentChordKeys[index] != requiredChordKeys[index])
            {
                return false;
            }
        }

        return true;
    }

    private static string GetChordKeySignature(IReadOnlyList<int> chordKeys)
    {
        return chordKeys.Count == 0
            ? string.Empty
            : string.Join(",", chordKeys);
    }

    private static int CountBits(ulong value)
    {
        int count = 0;
        while (value != 0)
        {
            value &= value - 1;
            count++;
        }

        return count;
    }

    public void Dispose()
    {
        StopMonitoring();
        GC.SuppressFinalize(this);
    }

    [DllImport("xinput1_4.dll", EntryPoint = "XInputGetState", CallingConvention = CallingConvention.StdCall)]
    private static extern int XInputGetState(uint dwUserIndex, out XInputState pState);

    [StructLayout(LayoutKind.Sequential)]
    private struct XInputState
    {
        public uint dwPacketNumber;
        public XInputGamepad Gamepad;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct XInputGamepad
    {
        public ushort wButtons;
        public byte bLeftTrigger;
        public byte bRightTrigger;
        public short sThumbLX;
        public short sThumbLY;
        public short sThumbRX;
        public short sThumbRY;
    }
}

public enum InputDeviceKind
{
    KeyboardMouse = 0,
    Gamepad = 1
}

public enum GamepadButtonId
{
    None = 0,
    A = 1,
    B = 2,
    X = 3,
    Y = 4,
    LeftShoulder = 5,
    RightShoulder = 6,
    LeftTrigger = 7,
    RightTrigger = 8,
    View = 9,
    Menu = 10,
    DPadUp = 11,
    DPadDown = 12,
    DPadLeft = 13,
    DPadRight = 14,
    LeftThumbstick = 15,
    RightThumbstick = 16
}

public static class InputBindingDisplay
{
    public static string GetDisplayText(HotkeyBindingSettings binding)
    {
        return GetDisplayText(binding.DeviceKind, binding.KeyCode, binding.Modifiers, binding.ChordKeyCodes, binding.GamepadButtonsMask, binding.GamepadButton);
    }

    public static string GetDisplayText(HoldHotkeyBindingSettings binding)
    {
        return GetDisplayText(binding.DeviceKind, binding.KeyCode, binding.Modifiers, binding.ChordKeyCodes, binding.GamepadButtonsMask, binding.GamepadButton);
    }

    public static string GetDisplayText(
        InputDeviceKind deviceKind,
        int keyCode,
        ModifierKeys modifiers,
        IReadOnlyList<int>? chordKeyCodes,
        ulong gamepadButtonsMask,
        GamepadButtonId gamepadButton)
    {
        return deviceKind switch
        {
            InputDeviceKind.Gamepad when gamepadButtonsMask != 0 || (chordKeyCodes?.Count ?? 0) > 0
                => GetGamepadButtonsDisplayString(gamepadButtonsMask != 0 ? gamepadButtonsMask : ToMask(gamepadButton), chordKeyCodes),
            InputDeviceKind.Gamepad when gamepadButton != GamepadButtonId.None => GetGamepadButtonsDisplayString(ToMask(gamepadButton), chordKeyCodes),
            _ when keyCode > 0 || modifiers != ModifierKeys.None || (chordKeyCodes?.Count ?? 0) > 0
                => GetKeyboardHotkeyDisplayString(keyCode, modifiers, chordKeyCodes),
            _ => string.Empty
        };
    }

    public static string GetKeyboardHotkeyDisplayString(int keyCode, ModifierKeys modifiers, IReadOnlyList<int>? chordKeyCodes)
    {
        List<string> parts = [];

        if (modifiers.HasFlag(ModifierKeys.Ctrl)) parts.Add(AppResources.GetString("Hotkeys.Modifier.Ctrl"));
        if (modifiers.HasFlag(ModifierKeys.Alt)) parts.Add(AppResources.GetString("Hotkeys.Modifier.Alt"));
        if (modifiers.HasFlag(ModifierKeys.Shift)) parts.Add(AppResources.GetString("Hotkeys.Modifier.Shift"));
        if (modifiers.HasFlag(ModifierKeys.Win)) parts.Add(AppResources.GetString("Hotkeys.Modifier.Win"));

        if (chordKeyCodes != null)
        {
            parts.AddRange(chordKeyCodes
                .Distinct()
                .OrderBy(code => code)
                .Select(VirtualKeys.GetKeyName));
        }

        if (keyCode > 0)
        {
            parts.Add(VirtualKeys.GetKeyName(keyCode));
        }

        return parts.Count > 0 ? string.Join(" + ", parts) : string.Empty;
    }

    public static string GetGamepadButtonsDisplayString(ulong buttonsMask, IReadOnlyList<int>? chordKeyCodes = null)
    {
        List<string> names = [];

        if (chordKeyCodes != null)
        {
            names.AddRange(chordKeyCodes
                .Distinct()
                .OrderBy(code => code)
                .Select(VirtualKeys.GetKeyName));
        }

        names.AddRange(GetButtonsFromMask(buttonsMask)
            .Select(GetGamepadButtonName)
            .Where(name => !string.IsNullOrEmpty(name)));

        return names.Count > 0 ? string.Join(" + ", names) : string.Empty;
    }

    public static ulong ToMask(GamepadButtonId button)
    {
        return button == GamepadButtonId.None
            ? 0
            : 1UL << (((int)button) - 1);
    }

    public static ulong ToMask(IEnumerable<GamepadButtonId> buttons)
    {
        ulong mask = 0;
        foreach (var button in buttons)
        {
            mask |= ToMask(button);
        }

        return mask;
    }

    public static IReadOnlyList<GamepadButtonId> GetButtonsFromMask(ulong buttonsMask)
    {
        List<GamepadButtonId> buttons = [];
        foreach (var button in Enum.GetValues<GamepadButtonId>())
        {
            if (button == GamepadButtonId.None)
            {
                continue;
            }

            if ((buttonsMask & ToMask(button)) != 0)
            {
                buttons.Add(button);
            }
        }

        return buttons;
    }

    public static string GetGamepadButtonName(GamepadButtonId button)
    {
        return button switch
        {
            GamepadButtonId.A => AppResources.GetString("Hotkeys.Key.GamepadA"),
            GamepadButtonId.B => AppResources.GetString("Hotkeys.Key.GamepadB"),
            GamepadButtonId.X => AppResources.GetString("Hotkeys.Key.GamepadX"),
            GamepadButtonId.Y => AppResources.GetString("Hotkeys.Key.GamepadY"),
            GamepadButtonId.LeftShoulder => AppResources.GetString("Hotkeys.Key.GamepadLeftShoulder"),
            GamepadButtonId.RightShoulder => AppResources.GetString("Hotkeys.Key.GamepadRightShoulder"),
            GamepadButtonId.LeftTrigger => AppResources.GetString("Hotkeys.Key.GamepadLeftTrigger"),
            GamepadButtonId.RightTrigger => AppResources.GetString("Hotkeys.Key.GamepadRightTrigger"),
            GamepadButtonId.View => AppResources.GetString("Hotkeys.Key.GamepadView"),
            GamepadButtonId.Menu => AppResources.GetString("Hotkeys.Key.GamepadMenu"),
            GamepadButtonId.DPadUp => AppResources.GetString("Hotkeys.Key.GamepadDPadUp"),
            GamepadButtonId.DPadDown => AppResources.GetString("Hotkeys.Key.GamepadDPadDown"),
            GamepadButtonId.DPadLeft => AppResources.GetString("Hotkeys.Key.GamepadDPadLeft"),
            GamepadButtonId.DPadRight => AppResources.GetString("Hotkeys.Key.GamepadDPadRight"),
            GamepadButtonId.LeftThumbstick => AppResources.GetString("Hotkeys.Key.GamepadLeftThumbstick"),
            GamepadButtonId.RightThumbstick => AppResources.GetString("Hotkeys.Key.GamepadRightThumbstick"),
            _ => string.Empty
        };
    }
}
