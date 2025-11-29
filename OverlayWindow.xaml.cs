using Microsoft.UI;
using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using silence_.Services;
using System;
using System.Runtime.InteropServices;
using Windows.Graphics;
using WinRT.Interop;

namespace silence_;


public sealed partial class OverlayWindow : Window
{
    private AppWindow? _appWindow;
    private IntPtr _hwnd;
    
    private bool _isPositioning = false;
    private bool _isDragging = false;
    private POINT _dragCursorOffset; // Cursor offset from window top-left when drag started
    
    private bool _isContentReady = false;
    private bool _pendingShow = false;
    
    // Magnetic snap configuration
    private const double MagneticRange = 60; // Range where magnetic effect starts (pixels)
    private const double SnapThreshold = 8; // Distance to fully snap to axis
    
    // Window dimensions
    private const int IconOnlyWidth = 48;
    private const int IconOnlyHeight = 48;
    private const int ContentPadding = 24; // Horizontal padding when text is shown
    
    // Current dimensions (may change based on content)
    private int _currentOverlayWidth = IconOnlyWidth;
    private int _currentOverlayHeight = IconOnlyHeight;
    
    // Current state
    private bool _currentMuteState = false;

    private Microsoft.UI.Windowing.AppWindowPresenterKind _presenterKind = Microsoft.UI.Windowing.AppWindowPresenterKind.Overlapped;

    public OverlayWindow()
    {
        InitializeComponent();
        SetupWindow();
        
        // Set up drag handlers on the root element
        if (Content is UIElement root)
        {
            root.PointerPressed += RootGrid_PointerPressed;
            root.PointerMoved += RootGrid_PointerMoved;
            root.PointerReleased += RootGrid_PointerReleased;
            root.PointerCaptureLost += RootGrid_PointerCaptureLost;
            root.KeyDown += RootGrid_KeyDown;
        }
        
        // Wait for content to be ready before showing (prevents white flash on Win10)
        RootGrid.Loaded += OnContentLoaded;
    }
    
    private void OnContentLoaded(object sender, RoutedEventArgs e)
    {
        _isContentReady = true;
        RootGrid.Loaded -= OnContentLoaded;
        
        // If show was requested before content was ready, show now
        if (_pendingShow)
        {
            _pendingShow = false;
            ShowWindow(_hwnd, SW_SHOWNOACTIVATE);
        }
    }
    
    private void RootGrid_KeyDown(object sender, KeyRoutedEventArgs e)
    {
        if (_isPositioning && e.Key == Windows.System.VirtualKey.Escape)
        {
            App.Instance?.StopOverlayPositioning();
            e.Handled = true;
        }
    }
    

    private void SetupWindow()
    {
        _hwnd = WindowNative.GetWindowHandle(this);
        var windowId = Win32Interop.GetWindowIdFromWindow(_hwnd);
        _appWindow = AppWindow.GetFromWindowId(windowId);
        
        // Hide window immediately to prevent white flash while content loads
        _appWindow?.Hide();

        if (_appWindow != null)
        {
            _appWindow.Title = "silence! overlay";
            
            // Use Overlapped presenter to allow removing borders
            _appWindow.SetPresenter(_presenterKind);

            // Set initial size
            _appWindow.Resize(new SizeInt32(_currentOverlayWidth, _currentOverlayHeight));
            
            // Remove title bar and borders
            if (_appWindow.Presenter is Microsoft.UI.Windowing.OverlappedPresenter overlappedPresenter)
            {
                overlappedPresenter.IsResizable = false;
                overlappedPresenter.IsMaximizable = false;
                overlappedPresenter.IsMinimizable = false;
                overlappedPresenter.SetBorderAndTitleBar(false, false);
            }

            // Make title bar completely transparent
            if (Microsoft.UI.Windowing.AppWindowTitleBar.IsCustomizationSupported())
            {
                _appWindow.TitleBar.ExtendsContentIntoTitleBar = true;
                _appWindow.TitleBar.ButtonBackgroundColor = Colors.Transparent;
                _appWindow.TitleBar.ButtonInactiveBackgroundColor = Colors.Transparent;
                _appWindow.TitleBar.ButtonHoverBackgroundColor = Colors.Transparent;
                _appWindow.TitleBar.ButtonPressedBackgroundColor = Colors.Transparent;
                _appWindow.TitleBar.ForegroundColor = Colors.Transparent;
                _appWindow.TitleBar.InactiveForegroundColor = Colors.Transparent;
                _appWindow.TitleBar.BackgroundColor = Colors.Transparent;
                _appWindow.TitleBar.InactiveBackgroundColor = Colors.Transparent;
                _appWindow.TitleBar.IconShowOptions = IconShowOptions.HideIconAndSystemMenu;
            }
        }
        
        // Hook WndProc to handle minimum size
        // IMPORTANT: Keep delegate reference to prevent GC from collecting it!
        _wndProcDelegate = new WndProcDelegate(WndProc);
        _oldWndProc = SetWindowLongPtr(_hwnd, GWLP_WNDPROC, _wndProcDelegate);

        // Set Win32 styles for click-through and topmost
        SetWindowStyles();
        SetWindowTopmost(true);
        
        // Apply rounded corners only on Windows 11+
        ApplyPlatformStyles();
    }
    
    private void ApplyPlatformStyles()
    {
        // Windows 11 is build 22000+
        if (Environment.OSVersion.Version.Build >= 22000)
        {
            RootGrid.CornerRadius = new CornerRadius(6);
        }
    }
    
    private delegate IntPtr WndProcDelegate(IntPtr hwnd, uint message, IntPtr wParam, IntPtr lParam);
    private WndProcDelegate? _wndProcDelegate; // MUST keep reference to prevent GC!
    private IntPtr _oldWndProc;
    private const int GWLP_WNDPROC = -4;
    private const uint WM_GETMINMAXINFO = 0x0024;

    [StructLayout(LayoutKind.Sequential)]
    private struct MINMAXINFO
    {
        public PointInt32 ptReserved;
        public PointInt32 ptMaxSize;
        public PointInt32 ptMaxPosition;
        public PointInt32 ptMinTrackSize;
        public PointInt32 ptMaxTrackSize;
    }

    private IntPtr WndProc(IntPtr hwnd, uint message, IntPtr wParam, IntPtr lParam)
    {
        if (message == WM_GETMINMAXINFO)
        {
            var minMaxInfo = Marshal.PtrToStructure<MINMAXINFO>(lParam);
            minMaxInfo.ptMinTrackSize.X = _currentOverlayWidth;
            minMaxInfo.ptMinTrackSize.Y = _currentOverlayHeight;
            Marshal.StructureToPtr(minMaxInfo, lParam, true);
        }
        return CallWindowProc(_oldWndProc, hwnd, message, wParam, lParam);
    }

    [DllImport("user32.dll")]
    private static extern IntPtr SetWindowLongPtr(IntPtr hWnd, int nIndex, WndProcDelegate dwNewLong);

    [DllImport("user32.dll")]
    private static extern IntPtr CallWindowProc(IntPtr lpPrevWndFunc, IntPtr hWnd, uint Msg, IntPtr wParam, IntPtr lParam);

    private void SetWindowTopmost(bool topmost)
    {
        var hwndTopmost = new IntPtr(-1); // HWND_TOPMOST
        var hwndNoTopmost = new IntPtr(-2); // HWND_NOTOPMOST
        SetWindowPos(_hwnd, topmost ? hwndTopmost : hwndNoTopmost, 0, 0, 0, 0, 
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
    }

    private void SetWindowStyles()
    {
        // Remove window border/frame styles - make it a simple popup overlay
        var style = GetWindowLong(_hwnd, GWL_STYLE);
        SetWindowLong(_hwnd, GWL_STYLE, (style & ~(WS_CAPTION | WS_THICKFRAME | WS_SYSMENU | WS_BORDER)) | WS_POPUP);
        
        // Set extended window style: tool window (no taskbar), click-through, no activate
        var exStyle = GetWindowLong(_hwnd, GWL_EXSTYLE);
        SetWindowLong(_hwnd, GWL_EXSTYLE, exStyle | WS_EX_TRANSPARENT | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE);
        
        // Force window size after style changes - WinUI ignores small sizes otherwise
        // SWP_FRAMECHANGED is needed to apply style changes
        SetWindowPos(_hwnd, IntPtr.Zero, 0, 0, _currentOverlayWidth, _currentOverlayHeight, 
            SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED);
    }

    public void SetClickThrough(bool clickThrough)
    {
        var exStyle = GetWindowLong(_hwnd, GWL_EXSTYLE);
        
        if (clickThrough)
        {
            SetWindowLong(_hwnd, GWL_EXSTYLE, exStyle | WS_EX_TRANSPARENT);
        }
        else
        {
            SetWindowLong(_hwnd, GWL_EXSTYLE, exStyle & ~WS_EX_TRANSPARENT);
        }
    }

    public void UpdateMuteState(bool isMuted)
    {
        _currentMuteState = isMuted;
        DispatcherQueue.TryEnqueue(() =>
        {
            var settings = App.Instance?.SettingsService.Settings;
            ApplyOverlayStyle(isMuted, settings);
        });
    }
    
    private void ApplyOverlayStyle(bool isMuted, AppSettings? settings)
    {
        if (settings == null) return;
        
        bool isMonochrome = settings.OverlayIconStyle == "Monochrome";
        bool isDarkBackground = settings.OverlayBackgroundStyle == "Dark";
        bool showText = settings.OverlayShowText;
        
        // Set icon glyph - use mic-off icon when muted
        MicIcon.Glyph = isMuted ? "\uF781" : "\uE720"; // MicOff : Microphone
        
        // Set icon color based on style
        if (isMonochrome)
        {
            // Monochrome: white on dark, black on light
            var monoColor = isDarkBackground 
                ? Windows.UI.Color.FromArgb(255, 255, 255, 255)  // White
                : Windows.UI.Color.FromArgb(255, 0, 0, 0);       // Black
            MicIcon.Foreground = new SolidColorBrush(monoColor);
        }
        else
        {
            // Colored: red when muted, green when unmuted
            var color = isMuted
                ? Windows.UI.Color.FromArgb(255, 220, 53, 69)   // Red
                : Windows.UI.Color.FromArgb(255, 40, 167, 69);  // Green
            MicIcon.Foreground = new SolidColorBrush(color);
        }
        
        // Set background color
        var bgColor = isDarkBackground
            ? Windows.UI.Color.FromArgb(255, 30, 30, 30)        // Dark (#1E1E1E)
            : Windows.UI.Color.FromArgb(255, 255, 255, 255);    // Light (white)
        RootGrid.Background = new SolidColorBrush(bgColor);
        
        // Set text visibility and content
        StatusText.Visibility = showText ? Visibility.Visible : Visibility.Collapsed;
        StatusText.Text = isMuted ? "Microphone is muted" : "Microphone is unmuted";
        
        // Text color should contrast with background
        var textColor = isDarkBackground
            ? Windows.UI.Color.FromArgb(255, 255, 255, 255)     // White
            : Windows.UI.Color.FromArgb(255, 0, 0, 0);          // Black
        StatusText.Foreground = new SolidColorBrush(textColor);
        
        // Measure and resize window to fit content
        UpdateWindowSizeToFitContent(settings);
    }
    
    private void UpdateWindowSizeToFitContent(AppSettings settings)
    {
        if (_appWindow == null) return;
        
        int oldWidth = _currentOverlayWidth;
        
        if (!settings.OverlayShowText)
        {
            // Icon only - fixed size
            _currentOverlayWidth = IconOnlyWidth;
            _currentOverlayHeight = IconOnlyHeight;
        }
        else
        {
            // Measure text to calculate required width
            StatusText.Measure(new Windows.Foundation.Size(double.PositiveInfinity, double.PositiveInfinity));
            var textWidth = StatusText.DesiredSize.Width;
            
            // Icon (32px) + spacing (8px) + text + padding
            _currentOverlayWidth = (int)(32 + 8 + textWidth + ContentPadding);
            _currentOverlayHeight = IconOnlyHeight;
        }
        
        // Only resize if dimensions changed
        if (oldWidth != _currentOverlayWidth)
        {
            ResizeWindowWithAnchor(settings, oldWidth);
        }
    }
    
    private void ResizeWindowWithAnchor(AppSettings settings, int oldWidth)
    {
        if (_appWindow == null) return;
        
        var currentPos = _appWindow.Position;
        int widthDiff = _currentOverlayWidth - oldWidth;
        int newX = currentPos.X;
        
        // Calculate anchor based on position percentage
        // < 40% = left anchor, > 60% = right anchor, 40-60% = center anchor
        if (settings.OverlayPositionX > 60)
        {
            // Right anchor: expand left
            newX = currentPos.X - widthDiff;
        }
        else if (settings.OverlayPositionX >= 40)
        {
            // Center anchor: expand both sides
            newX = currentPos.X - widthDiff / 2;
        }
        // else: Left anchor: keep X position (expand right)
        
        _appWindow.Resize(new SizeInt32(_currentOverlayWidth, _currentOverlayHeight));
        _appWindow.Move(new PointInt32(newX, currentPos.Y));
        
        SetWindowPos(_hwnd, IntPtr.Zero, newX, currentPos.Y, _currentOverlayWidth, _currentOverlayHeight, 
            SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED);
    }
    
    public void ApplySettings()
    {
        var settings = App.Instance?.SettingsService.Settings;
        if (settings == null) return;
        
        // Apply visual style (this also handles window resizing)
        DispatcherQueue.TryEnqueue(() =>
        {
            ApplyOverlayStyle(_currentMuteState, settings);
        });
    }

    public void ShowOverlay()
    {
        if (_isContentReady)
        {
            ShowWindow(_hwnd, SW_SHOWNOACTIVATE);
        }
        else
        {
            // Content not ready yet, defer show until Loaded event
            _pendingShow = true;
        }
    }

    public void HideOverlay()
    {
        _appWindow?.Hide();
    }

    public void MoveToPosition(double percentX, double percentY, string screenId)
    {
        if (_appWindow == null) return;

        var workArea = GetTargetScreenWorkArea(screenId);
        
        // Calculate position from percentage
        int x = (int)(workArea.X + (workArea.Width - _currentOverlayWidth) * percentX / 100.0);
        int y = (int)(workArea.Y + (workArea.Height - _currentOverlayHeight) * percentY / 100.0);
        
        _appWindow.Move(new PointInt32(x, y));
    }

    private RectInt32 GetTargetScreenWorkArea(string screenId)
    {
        // If PRIMARY or empty, use primary monitor
        if (screenId == "PRIMARY" || string.IsNullOrEmpty(screenId))
        {
            return GetPrimaryMonitorWorkArea();
        }

        // Find monitor by device name using Win32 API
        RectInt32? foundWorkArea = null;
        
        EnumDisplayMonitors(IntPtr.Zero, IntPtr.Zero, (IntPtr hMonitor, IntPtr hdcMonitor, ref RECT lprcMonitor, IntPtr dwData) =>
        {
            var mi = new MONITORINFOEX();
            mi.cbSize = Marshal.SizeOf(mi);
            if (GetMonitorInfo(hMonitor, ref mi) && mi.szDevice == screenId)
            {
                foundWorkArea = new RectInt32(
                    mi.rcWork.Left,
                    mi.rcWork.Top,
                    mi.rcWork.Right - mi.rcWork.Left,
                    mi.rcWork.Bottom - mi.rcWork.Top
                );
                return false; // Stop enumeration
            }
            return true;
        }, IntPtr.Zero);

        return foundWorkArea ?? GetPrimaryMonitorWorkArea();
    }
    
    private RectInt32 GetPrimaryMonitorWorkArea()
    {
        RectInt32 primaryWorkArea = new RectInt32(0, 0, 1920, 1080); // Default fallback
        
        EnumDisplayMonitors(IntPtr.Zero, IntPtr.Zero, (IntPtr hMonitor, IntPtr hdcMonitor, ref RECT lprcMonitor, IntPtr dwData) =>
        {
            var mi = new MONITORINFOEX();
            mi.cbSize = Marshal.SizeOf(mi);
            if (GetMonitorInfo(hMonitor, ref mi) && (mi.dwFlags & MONITORINFOF_PRIMARY) != 0)
            {
                primaryWorkArea = new RectInt32(
                    mi.rcWork.Left,
                    mi.rcWork.Top,
                    mi.rcWork.Right - mi.rcWork.Left,
                    mi.rcWork.Bottom - mi.rcWork.Top
                );
                return false; // Stop enumeration
            }
            return true;
        }, IntPtr.Zero);

        return primaryWorkArea;
    }
    
    // Monitor enumeration
    private delegate bool MonitorEnumProc(IntPtr hMonitor, IntPtr hdcMonitor, ref RECT lprcMonitor, IntPtr dwData);
    
    [DllImport("user32.dll")]
    private static extern bool EnumDisplayMonitors(IntPtr hdc, IntPtr lprcClip, MonitorEnumProc lpfnEnum, IntPtr dwData);
    
    [DllImport("user32.dll", CharSet = CharSet.Auto)]
    private static extern bool GetMonitorInfo(IntPtr hMonitor, ref MONITORINFOEX lpmi);
    
    private const int MONITORINFOF_PRIMARY = 0x00000001;
    
    [StructLayout(LayoutKind.Sequential)]
    private struct RECT
    {
        public int Left;
        public int Top;
        public int Right;
        public int Bottom;
    }
    
    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Auto)]
    private struct MONITORINFOEX
    {
        public int cbSize;
        public RECT rcMonitor;
        public RECT rcWork;
        public int dwFlags;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 32)]
        public string szDevice;
    }

    public void StartPositioning()
    {
        _isPositioning = true;
        SetClickThrough(false);
        
        DispatcherQueue.TryEnqueue(() =>
        {
            PositionHintBorder.Visibility = Visibility.Visible;
            // Highlight the root border
            RootGrid.BorderBrush = new SolidColorBrush(Windows.UI.Color.FromArgb(255, 0, 120, 215)); // Accent blue
            RootGrid.BorderThickness = new Thickness(2);
        });
        
        ShowOverlay();
    }

    public void StopPositioning()
    {
        _isPositioning = false;
        _isDragging = false;
        SetClickThrough(true);
        
        DispatcherQueue.TryEnqueue(() =>
        {
            PositionHintBorder.Visibility = Visibility.Collapsed;
            RootGrid.BorderBrush = null;
            RootGrid.BorderThickness = new Thickness(0);
        });
        
        // Save position
        SaveCurrentPosition();
    }

    private void SaveCurrentPosition()
    {
        if (_appWindow == null) return;
        
        var settings = App.Instance?.SettingsService.Settings;
        if (settings == null) return;

        var workArea = GetTargetScreenWorkArea(settings.OverlayScreenId);
        var position = _appWindow.Position;

        // Convert position to percentage
        double percentX = (position.X - workArea.X) * 100.0 / (workArea.Width - _currentOverlayWidth);
        double percentY = (position.Y - workArea.Y) * 100.0 / (workArea.Height - _currentOverlayHeight);

        // Clamp values
        percentX = Math.Clamp(percentX, 0, 100);
        percentY = Math.Clamp(percentY, 0, 100);

        App.Instance?.SettingsService.UpdateOverlayPosition(percentX, percentY);
    }

    private void RootGrid_PointerPressed(object sender, PointerRoutedEventArgs e)
    {
        if (!_isPositioning) return;

        _isDragging = true;
        if (sender is UIElement element)
        {
            element.CapturePointer(e.Pointer);
        }
        
        // Get absolute cursor position and calculate offset from window top-left
        GetCursorPos(out POINT cursorPos);
        var windowPos = _appWindow?.Position ?? new PointInt32(0, 0);
        _dragCursorOffset = new POINT 
        { 
            X = cursorPos.X - windowPos.X, 
            Y = cursorPos.Y - windowPos.Y 
        };
        
        e.Handled = true;
    }

    private void RootGrid_PointerMoved(object sender, PointerRoutedEventArgs e)
    {
        if (!_isDragging || _appWindow == null) return;

        // Get absolute cursor position (screen coordinates)
        GetCursorPos(out POINT cursorPos);
        
        // Base position = cursor minus the offset we saved when drag started
        // This makes the window follow cursor exactly
        double baseX = cursorPos.X - _dragCursorOffset.X;
        double baseY = cursorPos.Y - _dragCursorOffset.Y;

        // Get work area and calculate center axes
        var settings = App.Instance?.SettingsService.Settings;
        var workArea = GetTargetScreenWorkArea(settings?.OverlayScreenId ?? "PRIMARY");
        
        double centerX = workArea.X + (workArea.Width - _currentOverlayWidth) / 2.0;
        double centerY = workArea.Y + (workArea.Height - _currentOverlayHeight) / 2.0;

        // Calculate distance from center axes
        double distanceFromCenterX = Math.Abs(baseX - centerX);
        double distanceFromCenterY = Math.Abs(baseY - centerY);

        double finalX = baseX;
        double finalY = baseY;

        // Smooth magnetic attraction to horizontal center axis
        if (distanceFromCenterX < MagneticRange)
        {
            if (distanceFromCenterX < SnapThreshold)
            {
                // Full snap when very close
                finalX = centerX;
            }
            else
            {
                // Smooth interpolation: strength increases as we get closer to axis
                // Using cubic easing for smoother feel
                double t = 1.0 - (distanceFromCenterX / MagneticRange);
                double strength = t * t * t; // Cubic easing
                finalX = baseX + (centerX - baseX) * strength;
            }
        }

        // Smooth magnetic attraction to vertical center axis
        if (distanceFromCenterY < MagneticRange)
        {
            if (distanceFromCenterY < SnapThreshold)
            {
                // Full snap when very close
                finalY = centerY;
            }
            else
            {
                // Smooth interpolation
                double t = 1.0 - (distanceFromCenterY / MagneticRange);
                double strength = t * t * t; // Cubic easing
                finalY = baseY + (centerY - baseY) * strength;
            }
        }

        _appWindow.Move(new PointInt32((int)Math.Round(finalX), (int)Math.Round(finalY)));
        e.Handled = true;
    }

    private void RootGrid_PointerReleased(object sender, PointerRoutedEventArgs e)
    {
        if (!_isDragging) return;

        _isDragging = false;
        if (sender is UIElement element)
        {
            element.ReleasePointerCapture(e.Pointer);
        }
        e.Handled = true;
    }

    private void RootGrid_PointerCaptureLost(object sender, PointerRoutedEventArgs e)
    {
        _isDragging = false;
    }

    // Win32 API imports
    private const int GWL_STYLE = -16;
    private const int GWL_EXSTYLE = -20;
    
    // Window styles
    private const int WS_POPUP = unchecked((int)0x80000000);
    private const int WS_BORDER = 0x00800000;
    private const int WS_CAPTION = 0x00C00000;
    private const int WS_THICKFRAME = 0x00040000;
    private const int WS_SYSMENU = 0x00080000;
    
    // Extended window styles
    private const int WS_EX_TRANSPARENT = 0x00000020;
    private const int WS_EX_TOOLWINDOW = 0x00000080;
    private const int WS_EX_NOACTIVATE = 0x08000000;
    
    
    // SetWindowPos flags
    private const uint SWP_NOMOVE = 0x0002;
    private const uint SWP_NOSIZE = 0x0001;
    private const uint SWP_NOACTIVATE = 0x0010;
    private const uint SWP_SHOWWINDOW = 0x0040;
    private const uint SWP_NOZORDER = 0x0004;
    private const uint SWP_FRAMECHANGED = 0x0020;

    // ShowWindow commands
    private const int SW_SHOWNOACTIVATE = 4;
    

    [StructLayout(LayoutKind.Sequential)]
    private struct POINT
    {
        public int X;
        public int Y;
    }
    
    [DllImport("user32.dll")]
    private static extern bool GetCursorPos(out POINT lpPoint);
    
    [DllImport("user32.dll")]
    private static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);

    [DllImport("user32.dll")]
    private static extern int GetWindowLong(IntPtr hwnd, int index);

    [DllImport("user32.dll")]
    private static extern int SetWindowLong(IntPtr hwnd, int index, int newStyle);
    
    [DllImport("user32.dll")]
    private static extern bool SetWindowPos(IntPtr hWnd, IntPtr hWndInsertAfter, int X, int Y, int cx, int cy, uint uFlags);
}
