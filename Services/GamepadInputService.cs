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
    private readonly Dictionary<Gamepad, HashSet<GamepadButtonId>> _previousPressedInputs = new();
    private readonly HashSet<Gamepad> _activeHoldSources = new();

    private Timer? _pollTimer;
    private bool _isHoldButtonPressed;
    private GamepadButtonId _activeHoldButton = GamepadButtonId.None;
    private Gamepad? _recordingSourceGamepad;
    private GamepadButtonId _recordingButtonCandidate = GamepadButtonId.None;
    private DateTime _recordingButtonStartTime;

    public event Action<string>? HotkeyPressed;
    public event Action? HoldHotkeyPressed;
    public event Action? HoldHotkeyReleased;
    public event Action<GamepadButtonId>? ButtonPressed;
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
            ResetActiveStateLocked();
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
            _previousPressedInputs.Clear();
            ResetActiveStateLocked();
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
            ResetActiveStateLocked();
        }
    }

    public void ResetRecordingState()
    {
        lock (_syncLock)
        {
            ResetRecordingStateLocked();
        }

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
            if (hotkey == null ||
                hotkey.DeviceKind != InputDeviceKind.Gamepad ||
                hotkey.GamepadButton == GamepadButtonId.None)
            {
                continue;
            }

            _hotkeys.Add(new HotkeyBindingSettings
            {
                Id = hotkey.Id,
                Action = hotkey.Action,
                DeviceKind = hotkey.DeviceKind,
                GamepadButton = hotkey.GamepadButton
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
            if (hotkey == null ||
                hotkey.DeviceKind != InputDeviceKind.Gamepad ||
                hotkey.GamepadButton == GamepadButtonId.None)
            {
                continue;
            }

            _holdHotkeys.Add(new HoldHotkeyBindingSettings
            {
                Id = hotkey.Id,
                DeviceKind = hotkey.DeviceKind,
                GamepadButton = hotkey.GamepadButton
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

        RemoveDisconnectedGamepadsLocked(connectedGamepads, pendingActions);

        foreach (var gamepad in connectedGamepads)
        {
            var currentPressedInputs = GetPressedInputs(gamepad.GetCurrentReading());
            var previousPressedInputs = _previousPressedInputs.TryGetValue(gamepad, out var storedInputs)
                ? storedInputs
                : EmptyPressedInputs();

            foreach (var input in SupportedInputs)
            {
                var buttonId = input.ButtonId;
                var wasPressed = previousPressedInputs.Contains(buttonId);
                var isPressed = currentPressedInputs.Contains(buttonId);

                if (!wasPressed && isPressed)
                {
                    HandleButtonDownLocked(gamepad, buttonId, pendingActions);
                }
                else if (wasPressed && !isPressed)
                {
                    HandleButtonUpLocked(gamepad, buttonId, pendingActions);
                }
            }

            _previousPressedInputs[gamepad] = currentPressedInputs;
        }

        if (IsRecording)
        {
            UpdateRecordingCandidateLocked(connectedGamepads, pendingActions);
        }

        return pendingActions;
    }

    private void HandleButtonDownLocked(Gamepad gamepad, GamepadButtonId buttonId, List<Action> pendingActions)
    {
        if (IsRecording)
        {
            return;
        }

        if (_isHoldButtonPressed)
        {
            if (buttonId == _activeHoldButton)
            {
                _activeHoldSources.Add(gamepad);
            }

            return;
        }

        if (TryHandleHoldHotkeyDownLocked(gamepad, buttonId, pendingActions))
        {
            return;
        }

        if (IsConflictingWithHoldHotkey(buttonId))
        {
            return;
        }

        foreach (var hotkey in _hotkeys)
        {
            if (hotkey.GamepadButton != buttonId)
            {
                continue;
            }

            var action = hotkey.Action;
            pendingActions.Add(() => HotkeyPressed?.Invoke(action));
            return;
        }
    }

    private void HandleButtonUpLocked(Gamepad gamepad, GamepadButtonId buttonId, List<Action> pendingActions)
    {
        if (IsRecording)
        {
            if (_recordingSourceGamepad == gamepad && _recordingButtonCandidate == buttonId)
            {
                ResetRecordingStateLocked();
                pendingActions.Add(() => ButtonHoldProgress?.Invoke(0));
            }

            return;
        }

        if (!_isHoldButtonPressed || buttonId != _activeHoldButton)
        {
            return;
        }

        _activeHoldSources.Remove(gamepad);
        if (_activeHoldSources.Count > 0)
        {
            return;
        }

        ResetActiveStateLocked();
        pendingActions.Add(() => HoldHotkeyReleased?.Invoke());
    }

    private void UpdateRecordingCandidateLocked(Gamepad[] connectedGamepads, List<Action> pendingActions)
    {
        if (_recordingButtonCandidate != GamepadButtonId.None && _recordingSourceGamepad != null)
        {
            if (!TryGetPressedInputs(_recordingSourceGamepad, out var pressedInputs) ||
                !pressedInputs.Contains(_recordingButtonCandidate))
            {
                ResetRecordingStateLocked();
                pendingActions.Add(() => ButtonHoldProgress?.Invoke(0));
                return;
            }

            var elapsed = (DateTime.UtcNow - _recordingButtonStartTime).TotalSeconds;
            var progress = Math.Min(elapsed / RecordingHoldDurationSeconds, 1.0);
            pendingActions.Add(() => ButtonHoldProgress?.Invoke(progress));

            if (progress >= 1.0)
            {
                var capturedButton = _recordingButtonCandidate;
                IsRecording = false;
                ResetRecordingStateLocked();
                pendingActions.Add(() => ButtonHoldProgress?.Invoke(0));
                pendingActions.Add(() => ButtonPressed?.Invoke(capturedButton));
            }

            return;
        }

        foreach (var gamepad in connectedGamepads)
        {
            if (!_previousPressedInputs.TryGetValue(gamepad, out var pressedInputs) || pressedInputs.Count == 0)
            {
                continue;
            }

            foreach (var input in SupportedInputs)
            {
                if (!pressedInputs.Contains(input.ButtonId))
                {
                    continue;
                }

                _recordingSourceGamepad = gamepad;
                _recordingButtonCandidate = input.ButtonId;
                _recordingButtonStartTime = DateTime.UtcNow;
                pendingActions.Add(() => ButtonHoldProgress?.Invoke(0));
                return;
            }
        }
    }

    private bool TryHandleHoldHotkeyDownLocked(Gamepad gamepad, GamepadButtonId buttonId, List<Action> pendingActions)
    {
        foreach (var holdHotkey in _holdHotkeys)
        {
            if (holdHotkey.GamepadButton != buttonId)
            {
                continue;
            }

            _isHoldButtonPressed = true;
            _activeHoldButton = buttonId;
            _activeHoldSources.Clear();
            _activeHoldSources.Add(gamepad);
            pendingActions.Add(() => HoldHotkeyPressed?.Invoke());
            return true;
        }

        return false;
    }

    private bool IsConflictingWithHoldHotkey(GamepadButtonId buttonId)
    {
        return _holdHotkeys.Any(hotkey => hotkey.GamepadButton == buttonId);
    }

    private void RemoveDisconnectedGamepadsLocked(IEnumerable<Gamepad> connectedGamepads, List<Action> pendingActions)
    {
        var connectedSet = connectedGamepads.ToHashSet();
        var disconnectedGamepads = _previousPressedInputs.Keys
            .Where(existing => !connectedSet.Contains(existing))
            .ToArray();

        foreach (var gamepad in disconnectedGamepads)
        {
            _previousPressedInputs.Remove(gamepad);

            if (_recordingSourceGamepad == gamepad)
            {
                ResetRecordingStateLocked();
                pendingActions.Add(() => ButtonHoldProgress?.Invoke(0));
            }

            if (!_activeHoldSources.Remove(gamepad) || _activeHoldSources.Count > 0)
            {
                continue;
            }

            if (_isHoldButtonPressed)
            {
                ResetActiveStateLocked();
                pendingActions.Add(() => HoldHotkeyReleased?.Invoke());
            }
        }
    }

    private void PrimeKnownGamepadsLocked()
    {
        _previousPressedInputs.Clear();

        foreach (var gamepad in Gamepad.Gamepads)
        {
            _previousPressedInputs[gamepad] = GetPressedInputs(gamepad.GetCurrentReading());
        }
    }

    private void ResetActiveStateLocked()
    {
        _isHoldButtonPressed = false;
        _activeHoldButton = GamepadButtonId.None;
        _activeHoldSources.Clear();
    }

    private void ResetRecordingStateLocked()
    {
        _recordingSourceGamepad = null;
        _recordingButtonCandidate = GamepadButtonId.None;
        _recordingButtonStartTime = default;
    }

    private static HashSet<GamepadButtonId> GetPressedInputs(GamepadReading reading)
    {
        var pressedInputs = new HashSet<GamepadButtonId>();

        foreach (var input in SupportedInputs)
        {
            if (input.IsPressed(reading))
            {
                pressedInputs.Add(input.ButtonId);
            }
        }

        return pressedInputs;
    }

    private bool TryGetPressedInputs(Gamepad gamepad, out HashSet<GamepadButtonId> pressedInputs)
    {
        if (_previousPressedInputs.TryGetValue(gamepad, out var existingPressedInputs))
        {
            pressedInputs = existingPressedInputs;
            return true;
        }

        pressedInputs = EmptyPressedInputs();
        return false;
    }

    private static HashSet<GamepadButtonId> EmptyPressedInputs()
    {
        return new HashSet<GamepadButtonId>();
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
        return GetDisplayText(binding.DeviceKind, binding.KeyCode, binding.Modifiers, binding.GamepadButton);
    }

    public static string GetDisplayText(HoldHotkeyBindingSettings binding)
    {
        return GetDisplayText(binding.DeviceKind, binding.KeyCode, binding.Modifiers, binding.GamepadButton);
    }

    public static string GetDisplayText(
        InputDeviceKind deviceKind,
        int keyCode,
        ModifierKeys modifiers,
        GamepadButtonId gamepadButton)
    {
        return deviceKind switch
        {
            InputDeviceKind.Gamepad when gamepadButton != GamepadButtonId.None => GetGamepadButtonName(gamepadButton),
            _ when keyCode > 0 || modifiers != ModifierKeys.None => VirtualKeys.GetHotkeyDisplayString(keyCode, modifiers),
            _ => string.Empty
        };
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
