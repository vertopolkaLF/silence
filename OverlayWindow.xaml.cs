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
    
    // Magnetic snap configuration
    private const double MagneticRange = 60; // Range where magnetic effect starts (pixels)
    private const double SnapThreshold = 8; // Distance to fully snap to axis
    
    // Window dimensions - fit icon only (48x48)
    private const int OverlayWidth = 48;
    private const int OverlayHeight = 48;

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

        if (_appWindow != null)
        {
            _appWindow.Title = "silence! overlay";
            
            // Use Overlapped presenter to allow removing borders
            _appWindow.SetPresenter(_presenterKind);

            // Set initial size
            _appWindow.Resize(new SizeInt32(OverlayWidth, OverlayHeight));
            
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
            minMaxInfo.ptMinTrackSize.X = OverlayWidth;
            minMaxInfo.ptMinTrackSize.Y = OverlayHeight;
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
        SetWindowPos(_hwnd, IntPtr.Zero, 0, 0, OverlayWidth, OverlayHeight, 
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
        DispatcherQueue.TryEnqueue(() =>
        {
            if (isMuted)
            {
                MicIcon.Glyph = "\uE720"; // Microphone icon
                MicIcon.Foreground = new SolidColorBrush(Windows.UI.Color.FromArgb(255, 220, 53, 69)); // Red
            }
            else
            {
                MicIcon.Glyph = "\uE720"; // Microphone icon
                MicIcon.Foreground = new SolidColorBrush(Windows.UI.Color.FromArgb(255, 40, 167, 69)); // Green
            }
        });
    }

    public void ShowOverlay()
    {
        ShowWindow(_hwnd, SW_SHOWNOACTIVATE);
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
        int x = (int)(workArea.X + (workArea.Width - OverlayWidth) * percentX / 100.0);
        int y = (int)(workArea.Y + (workArea.Height - OverlayHeight) * percentY / 100.0);
        
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
        double percentX = (position.X - workArea.X) * 100.0 / (workArea.Width - OverlayWidth);
        double percentY = (position.Y - workArea.Y) * 100.0 / (workArea.Height - OverlayHeight);

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
        
        double centerX = workArea.X + (workArea.Width - OverlayWidth) / 2.0;
        double centerY = workArea.Y + (workArea.Height - OverlayHeight) / 2.0;

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
