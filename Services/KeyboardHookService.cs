using System;
using System.Diagnostics;
using System.Runtime.InteropServices;

namespace silence_.Services;

/// <summary>
/// Low-level keyboard hook for global hotkeys with modifier support
/// </summary>
public class KeyboardHookService : IDisposable
{
    private const int WH_KEYBOARD_LL = 13;
    private const int WH_MOUSE_LL = 14;
    private const int WM_KEYDOWN = 0x0100;
    private const int WM_KEYUP = 0x0101;
    private const int WM_SYSKEYDOWN = 0x0104;
    private const int WM_SYSKEYUP = 0x0105;
    private const int WM_LBUTTONDOWN = 0x0201;
    private const int WM_LBUTTONUP = 0x0202;
    private const int WM_RBUTTONDOWN = 0x0204;
    private const int WM_RBUTTONUP = 0x0205;
    private const int WM_MBUTTONDOWN = 0x0207;
    private const int WM_MBUTTONUP = 0x0208;
    private const int WM_XBUTTONDOWN = 0x020B;
    private const int WM_XBUTTONUP = 0x020C;

    private delegate IntPtr LowLevelKeyboardProc(int nCode, IntPtr wParam, IntPtr lParam);
    private delegate IntPtr LowLevelMouseProc(int nCode, IntPtr wParam, IntPtr lParam);
    
    [DllImport("user32.dll", CharSet = CharSet.Auto, SetLastError = true)]
    private static extern IntPtr SetWindowsHookEx(int idHook, LowLevelKeyboardProc lpfn, IntPtr hMod, uint dwThreadId);

    [DllImport("user32.dll", CharSet = CharSet.Auto, SetLastError = true)]
    private static extern IntPtr SetWindowsHookEx(int idHook, LowLevelMouseProc lpfn, IntPtr hMod, uint dwThreadId);

    [DllImport("user32.dll", CharSet = CharSet.Auto, SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool UnhookWindowsHookEx(IntPtr hhk);

    [DllImport("user32.dll", CharSet = CharSet.Auto, SetLastError = true)]
    private static extern IntPtr CallNextHookEx(IntPtr hhk, int nCode, IntPtr wParam, IntPtr lParam);

    [DllImport("kernel32.dll", CharSet = CharSet.Auto, SetLastError = true)]
    private static extern IntPtr GetModuleHandle(string lpModuleName);

    [DllImport("user32.dll")]
    private static extern short GetAsyncKeyState(int vKey);

    [StructLayout(LayoutKind.Sequential)]
    private struct KBDLLHOOKSTRUCT
    {
        public int vkCode;
        public int scanCode;
        public int flags;
        public int time;
        public IntPtr dwExtraInfo;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct MSLLHOOKSTRUCT
    {
        public POINT pt;
        public int mouseData;
        public int flags;
        public int time;
        public IntPtr dwExtraInfo;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct POINT
    {
        public int x;
        public int y;
    }

    private IntPtr _hookId = IntPtr.Zero;
    private IntPtr _mouseHookId = IntPtr.Zero;
    private LowLevelKeyboardProc? _proc;
    private LowLevelMouseProc? _mouseProc;
    private int _targetKey;
    private ModifierKeys _targetModifiers;
    private bool _ignoreModifiers;
    private bool _isHooked;

    // Hold hotkey support
    private int _holdKey;
    private ModifierKeys _holdModifiers;
    private bool _ignoreHoldModifiers;
    private bool _isHoldKeyPressed;

    // Current modifier state for recording
    private ModifierKeys _currentModifiers;
    private DateTime _modifierHoldStartTime;
    private bool _isWaitingForModifierHold;
    private System.Timers.Timer? _modifierHoldTimer;
    private DateTime _mouseHoldStartTime;
    private bool _isWaitingForMouseHold;
    private int _pendingMouseHoldKey;
    private System.Timers.Timer? _mouseHoldTimer;

    public event Action? HotkeyPressed;
    public event Action? HoldHotkeyPressed;
    public event Action? HoldHotkeyReleased;
    public event Action<int, ModifierKeys>? KeyPressed; // For hotkey recording (key + modifiers)
    public event Action<ModifierKeys>? ModifiersChanged; // For live modifier display
    public event Action<double>? ModifierHoldProgress; // Progress of modifier hold (0.0 to 1.0)

    public bool IsRecording { get; set; }

    public void StartHook(int virtualKeyCode, ModifierKeys modifiers, bool ignoreModifiers = true, 
        int holdKeyCode = 0, ModifierKeys holdModifiers = ModifierKeys.None, bool ignoreHoldModifiers = true)
    {
        StopHook();
        
        _targetKey = virtualKeyCode;
        _targetModifiers = modifiers;
        _ignoreModifiers = ignoreModifiers;
        _holdKey = holdKeyCode;
        _holdModifiers = holdModifiers;
        _ignoreHoldModifiers = ignoreHoldModifiers;
        _isHoldKeyPressed = false;
        _proc = HookCallback;
        _mouseProc = MouseHookCallback;
        
        using var curProcess = Process.GetCurrentProcess();
        using var curModule = curProcess.MainModule;
        
        if (curModule != null)
        {
            _hookId = SetWindowsHookEx(WH_KEYBOARD_LL, _proc, GetModuleHandle(curModule.ModuleName), 0);
            _mouseHookId = SetWindowsHookEx(WH_MOUSE_LL, _mouseProc, GetModuleHandle(curModule.ModuleName), 0);
            _isHooked = _hookId != IntPtr.Zero || _mouseHookId != IntPtr.Zero;
        }
    }

    public void StopHook()
    {
        if (_hookId != IntPtr.Zero)
        {
            UnhookWindowsHookEx(_hookId);
            _hookId = IntPtr.Zero;
        }
        if (_mouseHookId != IntPtr.Zero)
        {
            UnhookWindowsHookEx(_mouseHookId);
            _mouseHookId = IntPtr.Zero;
        }
        _isHooked = false;
    }

    public void UpdateHotkey(int virtualKeyCode, ModifierKeys modifiers, bool ignoreModifiers = true)
    {
        _targetKey = virtualKeyCode;
        _targetModifiers = modifiers;
        _ignoreModifiers = ignoreModifiers;
    }

    public void UpdateHoldHotkey(int virtualKeyCode, ModifierKeys modifiers, bool ignoreModifiers = true)
    {
        _holdKey = virtualKeyCode;
        _holdModifiers = modifiers;
        _ignoreHoldModifiers = ignoreModifiers;
        _isHoldKeyPressed = false;
    }

    private IntPtr HookCallback(int nCode, IntPtr wParam, IntPtr lParam)
    {
        if (nCode >= 0)
        {
            var hookStruct = Marshal.PtrToStructure<KBDLLHOOKSTRUCT>(lParam);
            var vkCode = hookStruct.vkCode;
            var isKeyDown = wParam == (IntPtr)WM_KEYDOWN || wParam == (IntPtr)WM_SYSKEYDOWN;
            var isKeyUp = wParam == (IntPtr)WM_KEYUP || wParam == (IntPtr)WM_SYSKEYUP;

            if (IsRecording)
            {
                // Handle modifier tracking for recording
                if (IsModifierKey(vkCode))
                {
                    if (isKeyDown)
                    {
                        var oldModifiers = _currentModifiers;
                        _currentModifiers |= VkCodeToModifier(vkCode);
                        
                        // Start timer if we just got 2+ modifiers
                        if (CountModifiers(_currentModifiers) >= 2 && CountModifiers(oldModifiers) < 2)
                        {
                            _modifierHoldStartTime = DateTime.Now;
                            _isWaitingForModifierHold = true;
                            StartModifierHoldTimer();
                        }
                    }
                    else if (isKeyUp)
                    {
                        _currentModifiers &= ~VkCodeToModifier(vkCode);
                        
                        // Stop timer if we no longer have 2+ modifiers
                        if (CountModifiers(_currentModifiers) < 2)
                        {
                            _isWaitingForModifierHold = false;
                            StopModifierHoldTimer();
                        }
                    }
                    ModifiersChanged?.Invoke(_currentModifiers);
                }
                else if (isKeyDown)
                {
                    RecordNonModifierKey(vkCode);
                }
            }
            else
            {
                if (isKeyDown)
                {
                    HandleKeyDown(vkCode);
                }
                else if (isKeyUp)
                {
                    HandleKeyUp(vkCode);
                }
            }
        }

        return CallNextHookEx(_hookId, nCode, wParam, lParam);
    }

    private IntPtr MouseHookCallback(int nCode, IntPtr wParam, IntPtr lParam)
    {
        if (nCode >= 0)
        {
            var message = (int)wParam;
            var isButtonDown = message == WM_LBUTTONDOWN || message == WM_RBUTTONDOWN || message == WM_MBUTTONDOWN || message == WM_XBUTTONDOWN;
            var isButtonUp = message == WM_LBUTTONUP || message == WM_RBUTTONUP || message == WM_MBUTTONUP || message == WM_XBUTTONUP;
            int vkCode = 0;

            if (message == WM_LBUTTONDOWN || message == WM_LBUTTONUP)
            {
                vkCode = VirtualKeys.LButton;
            }
            else if (message == WM_RBUTTONDOWN || message == WM_RBUTTONUP)
            {
                vkCode = VirtualKeys.RButton;
            }
            else if (message == WM_MBUTTONDOWN || message == WM_MBUTTONUP)
            {
                vkCode = VirtualKeys.MButton;
            }
            else if (message == WM_XBUTTONDOWN || message == WM_XBUTTONUP)
            {
                var hookStruct = Marshal.PtrToStructure<MSLLHOOKSTRUCT>(lParam);
                var xButton = (hookStruct.mouseData >> 16) & 0xFFFF;
                vkCode = xButton == 2 ? VirtualKeys.XButton2 : VirtualKeys.XButton1;
            }

            if (vkCode != 0)
            {
                if (IsRecording && isButtonDown)
                {
                    if (vkCode is VirtualKeys.LButton or VirtualKeys.RButton)
                    {
                        StartMouseHoldTimer(vkCode);
                    }
                    else
                    {
                        RecordNonModifierKey(vkCode);
                    }
                }
                else if (IsRecording && isButtonUp)
                {
                    if (_isWaitingForMouseHold && vkCode == _pendingMouseHoldKey)
                    {
                        _isWaitingForMouseHold = false;
                        _pendingMouseHoldKey = 0;
                        StopMouseHoldTimer();
                    }
                }
                else if (!IsRecording)
                {
                    if (isButtonDown)
                    {
                        HandleKeyDown(vkCode);
                    }
                    else if (isButtonUp)
                    {
                        HandleKeyUp(vkCode);
                    }
                }
            }
        }

        return CallNextHookEx(_mouseHookId, nCode, wParam, lParam);
    }

    private void RecordNonModifierKey(int vkCode)
    {
        StopModifierHoldTimer();
        _isWaitingForModifierHold = false;
        var finalModifiers = _currentModifiers | GetCurrentModifiers();
        KeyPressed?.Invoke(vkCode, finalModifiers);
        _currentModifiers = ModifierKeys.None;
    }

    private void HandleKeyDown(int vkCode)
    {
        var currentMods = GetCurrentModifiers();
        if (IsModifierKey(vkCode))
        {
            currentMods |= VkCodeToModifier(vkCode);
        }
        
        bool holdHotkeyTriggered = false;
        
        if (_holdKey == 0 && _holdModifiers != ModifierKeys.None && !_isHoldKeyPressed && IsModifierKey(vkCode))
        {
            bool matches = _ignoreHoldModifiers 
                ? (currentMods & _holdModifiers) == _holdModifiers
                : currentMods == _holdModifiers;
            
            if (matches)
            {
                _isHoldKeyPressed = true;
                holdHotkeyTriggered = true;
                HoldHotkeyPressed?.Invoke();
            }
        }
        else if (vkCode == _holdKey && _holdKey > 0 && !_isHoldKeyPressed)
        {
            bool matches = _ignoreHoldModifiers 
                ? (currentMods & _holdModifiers) == _holdModifiers
                : currentMods == _holdModifiers;
            
            if (matches)
            {
                _isHoldKeyPressed = true;
                holdHotkeyTriggered = true;
                HoldHotkeyPressed?.Invoke();
            }
        }
        
        if (!holdHotkeyTriggered && _targetKey == 0 && _targetModifiers != ModifierKeys.None && IsModifierKey(vkCode))
        {
            bool matches = _ignoreModifiers 
                ? (currentMods & _targetModifiers) == _targetModifiers
                : currentMods == _targetModifiers;
            
            if (matches)
            {
                HotkeyPressed?.Invoke();
            }
        }
        else if (!holdHotkeyTriggered && vkCode == _targetKey && _targetKey > 0)
        {
            if (vkCode == _holdKey && _holdKey > 0)
            {
                bool wouldMatchHold = _ignoreHoldModifiers 
                    ? (currentMods & _holdModifiers) == _holdModifiers
                    : currentMods == _holdModifiers;
                
                if (wouldMatchHold)
                {
                    return;
                }
            }
            
            bool matches = _ignoreModifiers 
                ? (currentMods & _targetModifiers) == _targetModifiers
                : currentMods == _targetModifiers;
            
            if (matches)
            {
                HotkeyPressed?.Invoke();
            }
        }
    }

    private void HandleKeyUp(int vkCode)
    {
        if (_holdKey == 0 && _holdModifiers != ModifierKeys.None && _isHoldKeyPressed)
        {
            if (IsModifierKey(vkCode))
            {
                var releasedMod = VkCodeToModifier(vkCode);
                if (_holdModifiers.HasFlag(releasedMod))
                {
                    _isHoldKeyPressed = false;
                    HoldHotkeyReleased?.Invoke();
                }
            }
        }
        else if (vkCode == _holdKey && _holdKey > 0 && _isHoldKeyPressed)
        {
            _isHoldKeyPressed = false;
            HoldHotkeyReleased?.Invoke();
        }
    }

    private static ModifierKeys GetCurrentModifiers()
    {
        var mods = ModifierKeys.None;
        
        if ((GetAsyncKeyState(0x10) & 0x8000) != 0 || // VK_SHIFT
            (GetAsyncKeyState(0xA0) & 0x8000) != 0 || // VK_LSHIFT
            (GetAsyncKeyState(0xA1) & 0x8000) != 0)   // VK_RSHIFT
            mods |= ModifierKeys.Shift;
            
        if ((GetAsyncKeyState(0x11) & 0x8000) != 0 || // VK_CONTROL
            (GetAsyncKeyState(0xA2) & 0x8000) != 0 || // VK_LCONTROL
            (GetAsyncKeyState(0xA3) & 0x8000) != 0)   // VK_RCONTROL
            mods |= ModifierKeys.Ctrl;
            
        if ((GetAsyncKeyState(0x12) & 0x8000) != 0 || // VK_MENU (Alt)
            (GetAsyncKeyState(0xA4) & 0x8000) != 0 || // VK_LMENU
            (GetAsyncKeyState(0xA5) & 0x8000) != 0)   // VK_RMENU
            mods |= ModifierKeys.Alt;
            
        if ((GetAsyncKeyState(0x5B) & 0x8000) != 0 || // VK_LWIN
            (GetAsyncKeyState(0x5C) & 0x8000) != 0)   // VK_RWIN
            mods |= ModifierKeys.Win;

        return mods;
    }

    private static ModifierKeys VkCodeToModifier(int vkCode)
    {
        return vkCode switch
        {
            0x10 or 0xA0 or 0xA1 => ModifierKeys.Shift,   // Shift
            0x11 or 0xA2 or 0xA3 => ModifierKeys.Ctrl,    // Ctrl
            0x12 or 0xA4 or 0xA5 => ModifierKeys.Alt,     // Alt
            0x5B or 0x5C => ModifierKeys.Win,             // Win
            _ => ModifierKeys.None
        };
    }

    private static bool IsModifierKey(int vkCode)
    {
        return vkCode is 0x10 or 0x11 or 0x12 or 0x5B or 0x5C 
            or 0xA0 or 0xA1 or 0xA2 or 0xA3 or 0xA4 or 0xA5;
    }

    private static int CountModifiers(ModifierKeys modifiers)
    {
        int count = 0;
        if (modifiers.HasFlag(ModifierKeys.Shift)) count++;
        if (modifiers.HasFlag(ModifierKeys.Ctrl)) count++;
        if (modifiers.HasFlag(ModifierKeys.Alt)) count++;
        if (modifiers.HasFlag(ModifierKeys.Win)) count++;
        return count;
    }

    private void StartModifierHoldTimer()
    {
        StopModifierHoldTimer();
        
        _modifierHoldTimer = new System.Timers.Timer(50); // Update every 50ms
        _modifierHoldTimer.Elapsed += (s, e) =>
        {
            if (!_isWaitingForModifierHold || CountModifiers(_currentModifiers) < 2)
            {
                StopModifierHoldTimer();
                return;
            }
            
            var elapsed = (DateTime.Now - _modifierHoldStartTime).TotalSeconds;
            var progress = Math.Min(elapsed / 1.0, 1.0); // 1 second hold
            
            ModifierHoldProgress?.Invoke(progress);
            
            if (progress >= 1.0)
            {
                // Modifier-only binding complete
                var capturedModifiers = _currentModifiers;
                StopModifierHoldTimer();
                _isWaitingForModifierHold = false;
                _currentModifiers = ModifierKeys.None;
                KeyPressed?.Invoke(0, capturedModifiers); // 0 = modifier-only binding
            }
        };
        _modifierHoldTimer.AutoReset = true;
        _modifierHoldTimer.Start();
    }

    private void StopModifierHoldTimer()
    {
        if (_modifierHoldTimer != null)
        {
            _modifierHoldTimer.Stop();
            _modifierHoldTimer.Dispose();
            _modifierHoldTimer = null;
            ModifierHoldProgress?.Invoke(0);
        }
    }

    private void StartMouseHoldTimer(int vkCode)
    {
        StopMouseHoldTimer();
        _mouseHoldStartTime = DateTime.Now;
        _pendingMouseHoldKey = vkCode;
        _isWaitingForMouseHold = true;
        _mouseHoldTimer = new System.Timers.Timer(50);
        _mouseHoldTimer.Elapsed += (s, e) =>
        {
            if (!_isWaitingForMouseHold || _pendingMouseHoldKey == 0)
            {
                StopMouseHoldTimer();
                return;
            }
            
            var elapsed = (DateTime.Now - _mouseHoldStartTime).TotalSeconds;
            var progress = Math.Min(elapsed / 1.0, 1.0);
            ModifierHoldProgress?.Invoke(progress);
            if (elapsed >= 1.0)
            {
                var capturedKey = _pendingMouseHoldKey;
                _isWaitingForMouseHold = false;
                _pendingMouseHoldKey = 0;
                StopMouseHoldTimer();
                RecordNonModifierKey(capturedKey);
            }
        };
        _mouseHoldTimer.AutoReset = true;
        _mouseHoldTimer.Start();
    }

    private void StopMouseHoldTimer()
    {
        if (_mouseHoldTimer != null)
        {
            _mouseHoldTimer.Stop();
            _mouseHoldTimer.Dispose();
            _mouseHoldTimer = null;
            ModifierHoldProgress?.Invoke(0);
        }
    }

    public void ResetRecordingState()
    {
        _currentModifiers = ModifierKeys.None;
        _isWaitingForModifierHold = false;
        StopModifierHoldTimer();
        _isWaitingForMouseHold = false;
        _pendingMouseHoldKey = 0;
        StopMouseHoldTimer();
    }

    public bool IsHooked => _isHooked;

    public void Dispose()
    {
        StopHook();
        StopModifierHoldTimer();
    }
}

[Flags]
public enum ModifierKeys
{
    None = 0,
    Shift = 1,
    Ctrl = 2,
    Alt = 4,
    Win = 8
}

/// <summary>
/// Virtual key codes
/// </summary>
public static class VirtualKeys
{
    public const int LButton = 0x01;
    public const int RButton = 0x02;
    public const int MButton = 0x04;
    public const int XButton1 = 0x05;
    public const int XButton2 = 0x06;
    public const int F1 = 0x70;
    public const int F2 = 0x71;
    public const int F3 = 0x72;
    public const int F4 = 0x73;
    public const int F5 = 0x74;
    public const int F6 = 0x75;
    public const int F7 = 0x76;
    public const int F8 = 0x77;
    public const int F9 = 0x78;
    public const int F10 = 0x79;
    public const int F11 = 0x7A;
    public const int F12 = 0x7B;
    public const int F13 = 0x7C;
    public const int F14 = 0x7D;
    public const int F15 = 0x7E;
    public const int F16 = 0x7F;
    public const int F17 = 0x80;
    public const int F18 = 0x81;
    public const int F19 = 0x82;
    public const int F20 = 0x83;
    public const int F21 = 0x84;
    public const int F22 = 0x85;
    public const int F23 = 0x86;
    public const int F24 = 0x87;

    public const int Escape = 0x1B;
    public const int Space = 0x20;
    public const int Insert = 0x2D;
    public const int Delete = 0x2E;
    public const int Pause = 0x13;
    public const int ScrollLock = 0x91;
    public const int PrintScreen = 0x2C;
    public const int NumLock = 0x90;

    public static string GetKeyName(int vkCode)
    {
        return vkCode switch
        {
            0x01 => "Mouse Left",
            0x02 => "Mouse Right",
            0x04 => "Mouse Middle",
            0x05 => "Mouse 4",
            0x06 => "Mouse 5",
            >= 0x70 and <= 0x87 => $"F{vkCode - 0x70 + 1}",
            >= 0x30 and <= 0x39 => ((char)vkCode).ToString(),
            >= 0x41 and <= 0x5A => ((char)vkCode).ToString(),
            0x1B => "Escape",
            0x20 => "Space",
            0x2D => "Insert",
            0x2E => "Delete",
            0x13 => "Pause",
            0x91 => "Scroll Lock",
            0x2C => "Print Screen",
            0x90 => "Num Lock",
            0x6A => "Numpad *",
            0x6B => "Numpad +",
            0x6D => "Numpad -",
            0x6E => "Numpad .",
            0x6F => "Numpad /",
            >= 0x60 and <= 0x69 => $"Numpad {vkCode - 0x60}",
            0xC0 => "`",
            0xBD => "-",
            0xBB => "=",
            0xDB => "[",
            0xDD => "]",
            0xDC => "\\",
            0xBA => ";",
            0xDE => "'",
            0xBC => ",",
            0xBE => ".",
            0xBF => "/",
            _ => $"Key {vkCode}"
        };
    }

    public static string GetHotkeyDisplayString(int keyCode, ModifierKeys modifiers)
    {
        var parts = new System.Collections.Generic.List<string>();
        
        if (modifiers.HasFlag(ModifierKeys.Ctrl)) parts.Add("Ctrl");
        if (modifiers.HasFlag(ModifierKeys.Alt)) parts.Add("Alt");
        if (modifiers.HasFlag(ModifierKeys.Shift)) parts.Add("Shift");
        if (modifiers.HasFlag(ModifierKeys.Win)) parts.Add("Win");
        
        if (keyCode > 0)
            parts.Add(GetKeyName(keyCode));
        
        return parts.Count > 0 ? string.Join(" + ", parts) : "";
    }
}
