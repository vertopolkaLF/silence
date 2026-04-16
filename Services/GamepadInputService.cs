using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading;
using Windows.Gaming.Input;

namespace silence_.Services;

public class GamepadInputService : IDisposable
{
    private const int PollingIntervalMs = 33;
    private const double TriggerPressThreshold = 0.75;
    private const double RecordingHoldDurationSeconds = 1.0;

    private static readonly GamepadInputDescriptor[] SupportedInputs =
    [
        new(GamepadButtonId.A, reading => (reading.Buttons & GamepadButtons.A) != 0),
        new(GamepadButtonId.B, reading => (reading.Buttons & GamepadButtons.B) != 0),
        new(GamepadButtonId.X, reading => (reading.Buttons & GamepadButtons.X) != 0),
        new(GamepadButtonId.Y, reading => (reading.Buttons & GamepadButtons.Y) != 0),
        new(GamepadButtonId.LeftShoulder, reading => (reading.Buttons & GamepadButtons.LeftShoulder) != 0),
        new(GamepadButtonId.RightShoulder, reading => (reading.Buttons & GamepadButtons.RightShoulder) != 0),
        new(GamepadButtonId.LeftTrigger, reading => reading.LeftTrigger >= TriggerPressThreshold),
        new(GamepadButtonId.RightTrigger, reading => reading.RightTrigger >= TriggerPressThreshold),
        new(GamepadButtonId.View, reading => (reading.Buttons & GamepadButtons.View) != 0),
        new(GamepadButtonId.Menu, reading => (reading.Buttons & GamepadButtons.Menu) != 0),
        new(GamepadButtonId.DPadUp, reading => (reading.Buttons & GamepadButtons.DPadUp) != 0),
        new(GamepadButtonId.DPadDown, reading => (reading.Buttons & GamepadButtons.DPadDown) != 0),
        new(GamepadButtonId.DPadLeft, reading => (reading.Buttons & GamepadButtons.DPadLeft) != 0),
        new(GamepadButtonId.DPadRight, reading => (reading.Buttons & GamepadButtons.DPadRight) != 0),
        new(GamepadButtonId.LeftThumbstick, reading => (reading.Buttons & GamepadButtons.LeftThumbstick) != 0),
        new(GamepadButtonId.RightThumbstick, reading => (reading.Buttons & GamepadButtons.RightThumbstick) != 0)
    ];

    private readonly object _syncLock = new();
    private readonly List<HotkeyBindingSettings> _hotkeys = new();
    private readonly List<HoldHotkeyBindingSettings> _holdHotkeys = new();
    private readonly Dictionary<Gamepad, ulong> _previousPressedMasks = new();
    private readonly Func<IReadOnlyList<int>> _recordingChordKeysProvider;
    private readonly Func<IReadOnlyList<int>> _pressedChordKeysProvider;

    private Timer? _pollTimer;
    private ulong _activeHoldButtonsMask;
    private Gamepad? _activeHoldSourceGamepad;
    private List<int> _activeHoldChordKeys = new();
    private List<int> _previousPressedChordKeys = new();
    private Gamepad? _recordingSourceGamepad;
    private ulong _recordingButtonsMask;
    private List<int> _recordingChordKeys = new();
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

    public GamepadInputService(
        Func<IReadOnlyList<int>>? recordingChordKeysProvider = null,
        Func<IReadOnlyList<int>>? pressedChordKeysProvider = null)
    {
        _recordingChordKeysProvider = recordingChordKeysProvider ?? (() => Array.Empty<int>());
        _pressedChordKeysProvider = pressedChordKeysProvider ?? (() => Array.Empty<int>());
    }

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
            _previousPressedChordKeys = new List<int>();
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

        RecordingButtonsChanged?.Invoke(0, Array.Empty<int>());
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
            IReadOnlyList<int> chordKeyCodes = hotkey.ChordKeyCodes ?? new List<int>();
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
                ChordKeyCodes = chordKeyCodes.ToList()
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
            IReadOnlyList<int> chordKeyCodes = hotkey.ChordKeyCodes ?? new List<int>();
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
                ChordKeyCodes = chordKeyCodes.ToList()
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
        var pendingActions = new List<Action>();
        var connectedGamepads = Gamepad.Gamepads.ToArray();
        var currentMasks = new Dictionary<Gamepad, ulong>(connectedGamepads.Length);
        var previousMasks = new Dictionary<Gamepad, ulong>(connectedGamepads.Length);
        var currentChordKeys = NormalizeChordKeys(_pressedChordKeysProvider());
        var previousChordKeys = _previousPressedChordKeys.ToList();
        var recordingChordKeys = NormalizeChordKeys(_recordingChordKeysProvider());

        foreach (var gamepad in connectedGamepads)
        {
            currentMasks[gamepad] = GetPressedMask(gamepad.GetCurrentReading());
            _previousPressedMasks.TryGetValue(gamepad, out var previousMask);
            previousMasks[gamepad] = previousMask;
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

        _previousPressedChordKeys = currentChordKeys.ToList();

        return pendingActions;
    }

    private void HandleHoldStateLocked(
        Gamepad gamepad,
        ulong previousMask,
        ulong currentMask,
        IReadOnlyList<int> previousChordKeys,
        IReadOnlyList<int> currentChordKeys,
        List<Action> pendingActions)
    {
        if (_activeHoldSourceGamepad == gamepad && (_activeHoldButtonsMask != 0 || _activeHoldChordKeys.Count > 0))
        {
            if (currentMask != _activeHoldButtonsMask ||
                !ChordKeysEqual(currentChordKeys, _activeHoldChordKeys))
            {
                ResetActiveHoldLocked();
                pendingActions.Add(() => HoldHotkeyReleased?.Invoke());
            }

            return;
        }

        if (_activeHoldSourceGamepad != null)
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

        _activeHoldSourceGamepad = gamepad;
        _activeHoldButtonsMask = activatedHold.Mask;
        _activeHoldChordKeys = activatedHold.Hotkey.ChordKeyCodes.ToList();
        pendingActions.Add(() => HoldHotkeyPressed?.Invoke());
    }

    private void HandleChordOnlyHoldStateLocked(
        IReadOnlyList<int> previousChordKeys,
        IReadOnlyList<int> currentChordKeys,
        List<Action> pendingActions)
    {
        if (_activeHoldSourceGamepad == null && _activeHoldButtonsMask == 0 && _activeHoldChordKeys.Count > 0)
        {
            if (!ChordKeysEqual(currentChordKeys, _activeHoldChordKeys))
            {
                ResetActiveHoldLocked();
                pendingActions.Add(() => HoldHotkeyReleased?.Invoke());
            }

            return;
        }

        if (_activeHoldSourceGamepad != null)
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

        _activeHoldSourceGamepad = null;
        _activeHoldButtonsMask = 0;
        _activeHoldChordKeys = activatedHold.ChordKeyCodes.ToList();
        pendingActions.Add(() => HoldHotkeyPressed?.Invoke());
    }

    private void TriggerHotkeysLocked(
        Dictionary<Gamepad, ulong> currentMasks,
        Dictionary<Gamepad, ulong> previousMasks,
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

            if (_activeHoldSourceGamepad == pair.Key && currentMask == _activeHoldButtonsMask)
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
        Dictionary<Gamepad, ulong> currentMasks,
        IReadOnlyList<int> recordingChordKeys,
        List<Action> pendingActions)
    {
        var currentCombinedMask = currentMasks.Values.Aggregate(0UL, static (current, next) => current | next);

        if (_recordingButtonsMask != 0 || _recordingChordKeys.Count > 0)
        {
            if (currentCombinedMask == 0 && recordingChordKeys.Count == 0)
            {
                ResetRecordingStateLocked();
                QueueRecordingUiUpdate(pendingActions, 0, Array.Empty<int>(), 0);
                return;
            }

            if (currentCombinedMask != _recordingButtonsMask ||
                !ChordKeysEqual(recordingChordKeys, _recordingChordKeys))
            {
                _recordingButtonsMask = currentCombinedMask;
                _recordingChordKeys = recordingChordKeys.ToList();
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
            var capturedChordKeys = _recordingChordKeys.ToList();
            IsRecording = false;
            ResetRecordingStateLocked();
            QueueRecordingUiUpdate(pendingActions, 0, Array.Empty<int>(), 0);
            pendingActions.Add(() => ButtonsCaptured?.Invoke(capturedMask, capturedChordKeys));
            return;
        }

        if (currentCombinedMask == 0 && recordingChordKeys.Count == 0)
        {
            QueueRecordingUiUpdate(pendingActions, 0, Array.Empty<int>(), 0);
            return;
        }

        _recordingSourceGamepad = null;
        _recordingButtonsMask = currentCombinedMask;
        _recordingChordKeys = recordingChordKeys.ToList();
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
            var chordKeysSnapshot = previewChordKeys.ToList();
            pendingActions.Add(() => RecordingButtonsChanged?.Invoke(previewMask, chordKeysSnapshot));
        }

        pendingActions.Add(() => ButtonHoldProgress?.Invoke(progress));
    }

    private void RemoveDisconnectedGamepadsLocked(IEnumerable<Gamepad> connectedGamepads, List<Action> pendingActions)
    {
        var connectedSet = connectedGamepads.ToHashSet();
        var disconnectedGamepads = _previousPressedMasks.Keys
            .Where(existing => !connectedSet.Contains(existing))
            .ToArray();

        foreach (var gamepad in disconnectedGamepads)
        {
            _previousPressedMasks.Remove(gamepad);

            if (_recordingSourceGamepad == gamepad)
            {
                ResetRecordingStateLocked();
                QueueRecordingUiUpdate(pendingActions, 0, Array.Empty<int>(), 0);
            }

            if (_activeHoldSourceGamepad == gamepad)
            {
                ResetActiveHoldLocked();
                pendingActions.Add(() => HoldHotkeyReleased?.Invoke());
            }
        }
    }

    private void PrimeKnownGamepadsLocked()
    {
        _previousPressedMasks.Clear();
        _previousPressedChordKeys = new List<int>();

        foreach (var gamepad in Gamepad.Gamepads)
        {
            _previousPressedMasks[gamepad] = GetPressedMask(gamepad.GetCurrentReading());
        }
    }

    private void ResetActiveHoldLocked()
    {
        _activeHoldButtonsMask = 0;
        _activeHoldSourceGamepad = null;
        _activeHoldChordKeys = new List<int>();
    }

    private void ResetRecordingStateLocked()
    {
        _recordingSourceGamepad = null;
        _recordingButtonsMask = 0;
        _recordingChordKeys = new List<int>();
        _recordingButtonsStartTime = default;
        _lastPreviewMask = 0;
        _lastPreviewChordSignature = string.Empty;
    }

    private static ulong GetPressedMask(GamepadReading reading)
    {
        ulong mask = 0;

        foreach (var input in SupportedInputs)
        {
            if (!input.IsPressed(reading))
            {
                continue;
            }

            mask |= InputBindingDisplay.ToMask(input.ButtonId);
        }

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

    private static IReadOnlyList<int> NormalizeChordKeys(IReadOnlyList<int> chordKeys)
    {
        return chordKeys
            .Where(VirtualKeys.IsChordableGenericKey)
            .Distinct()
            .OrderBy(code => code)
            .ToList();
    }

    private static bool ChordKeysEqual(IReadOnlyList<int> currentChordKeys, IReadOnlyList<int>? requiredChordKeys)
    {
        requiredChordKeys ??= Array.Empty<int>();
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
    }

    private sealed class GamepadInputDescriptor
    {
        public GamepadInputDescriptor(GamepadButtonId buttonId, Func<GamepadReading, bool> isPressed)
        {
            ButtonId = buttonId;
            IsPressed = isPressed;
        }

        public GamepadButtonId ButtonId { get; }
        public Func<GamepadReading, bool> IsPressed { get; }
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
        var parts = new List<string>();

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
        var names = new List<string>();

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
        var buttons = new List<GamepadButtonId>();
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
