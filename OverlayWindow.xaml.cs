using Microsoft.UI;
using Microsoft.UI.Composition;
using Microsoft.UI.Composition.SystemBackdrops;
using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using silence_.Services;
using System;
using System.Runtime.InteropServices;
using Windows.Graphics;
using WinRT;
using WinRT.Interop;

namespace silence_;


public sealed partial class OverlayWindow : Window
{
    private AppWindow? _appWindow;
    private IntPtr _hwnd;
    private DesktopAcrylicController? _acrylicController;
    private SystemBackdropConfiguration? _configurationSource;
    
    private bool _isPositioning = false;
    private bool _isDragging = false;
    private Windows.Foundation.Point _dragStartPoint;
    private PointInt32 _windowStartPosition;
    
    // Snap threshold in pixels
    private const int SnapThreshold = 30;
    
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
        
        // Setup acrylic backdrop for blur effect
        SetupAcrylicBackdrop();
    }
    
    private void SetupAcrylicBackdrop()
    {
        if (!DesktopAcrylicController.IsSupported()) return;
        
        _configurationSource = new SystemBackdropConfiguration();
        
        this.Activated += (s, e) =>
        {
            if (_configurationSource != null)
            {
                _configurationSource.IsInputActive = e.WindowActivationState != WindowActivationState.Deactivated;
            }
        };
        
        _acrylicController = new DesktopAcrylicController
        {
            Kind = DesktopAcrylicKind.Base,
            TintColor = Windows.UI.Color.FromArgb(255, 30, 30, 30),
            TintOpacity = 0.5f,
            LuminosityOpacity = 0.2f,
            FallbackColor = Windows.UI.Color.FromArgb(255, 30, 30, 30)
        };
        
        _acrylicController.AddSystemBackdropTarget(this.As<ICompositionSupportsSystemBackdrop>());
        _acrylicController.SetSystemBackdropConfiguration(_configurationSource);
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
        // Remove window border/frame styles completely
        var style = GetWindowLong(_hwnd, GWL_STYLE);
        SetWindowLong(_hwnd, GWL_STYLE, (style & ~(WS_CAPTION | WS_THICKFRAME | WS_SYSMENU | WS_BORDER)) | WS_POPUP);
        
        // Set extended window style: tool window (no taskbar), click-through, no activate
        var exStyle = GetWindowLong(_hwnd, GWL_EXSTYLE);
        SetWindowLong(_hwnd, GWL_EXSTYLE, exStyle | WS_EX_TRANSPARENT | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE);
        
        // Disable DWM window frame
        int value = 2; // DWMNCRP_DISABLED
        DwmSetWindowAttribute(_hwnd, DWMWA_NCRENDERING_POLICY, ref value, sizeof(int));

        // Remove DWM border color (Windows 11 specific)
        int colorNone = DWMWA_COLOR_NONE; 
        DwmSetWindowAttribute(_hwnd, DWMWA_BORDER_COLOR, ref colorNone, sizeof(int));
        
        // Disable caption color to remove any remaining border
        DwmSetWindowAttribute(_hwnd, DWMWA_CAPTION_COLOR, ref colorNone, sizeof(int));
        
        // Set window corner preference to round (Windows 11 handles anti-aliasing beautifully)
        int cornerPref = DWMWCP_ROUND;
        DwmSetWindowAttribute(_hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, ref cornerPref, sizeof(int));
        
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
        try
        {
            if (screenId == "PRIMARY" || string.IsNullOrEmpty(screenId))
            {
                var primary = DisplayArea.Primary;
                return primary.WorkArea;
            }

            // Try to find the specific display
            var displays = DisplayArea.FindAll();
            foreach (var display in displays)
            {
                if (display.DisplayId.Value.ToString() == screenId)
                {
                    return display.WorkArea;
                }
            }
        }
        catch
        {
            // Fallback
        }

        // Fallback to primary
        return DisplayArea.Primary.WorkArea;
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
            _dragStartPoint = e.GetCurrentPoint(element).Position;
            element.CapturePointer(e.Pointer);
        }
        _windowStartPosition = _appWindow?.Position ?? new PointInt32(0, 0);
        e.Handled = true;
    }

    private void RootGrid_PointerMoved(object sender, PointerRoutedEventArgs e)
    {
        if (!_isDragging || _appWindow == null || sender is not UIElement element) return;

        var currentPoint = e.GetCurrentPoint(element).Position;
        var deltaX = (int)(currentPoint.X - _dragStartPoint.X);
        var deltaY = (int)(currentPoint.Y - _dragStartPoint.Y);

        var newX = _windowStartPosition.X + deltaX;
        var newY = _windowStartPosition.Y + deltaY;

        // Apply magnetic snapping to center
        var settings = App.Instance?.SettingsService.Settings;
        var workArea = GetTargetScreenWorkArea(settings?.OverlayScreenId ?? "PRIMARY");
        
        int centerX = workArea.X + (workArea.Width - OverlayWidth) / 2;
        int centerY = workArea.Y + (workArea.Height - OverlayHeight) / 2;

        // Snap to horizontal center
        if (Math.Abs(newX - centerX) < SnapThreshold)
        {
            newX = centerX;
        }

        // Snap to vertical center
        if (Math.Abs(newY - centerY) < SnapThreshold)
        {
            newY = centerY;
        }

        _appWindow.Move(new PointInt32(newX, newY));
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
    private const int WS_EX_LAYERED = 0x00080000;
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
    
    // DWM attributes
    private const int DWMWA_NCRENDERING_ENABLED = 1;
    private const int DWMWA_NCRENDERING_POLICY = 2;
    private const int DWMWA_WINDOW_CORNER_PREFERENCE = 33;
    private const int DWMWA_BORDER_COLOR = 34;
    private const int DWMWA_CAPTION_COLOR = 35;
    private const int DWMWA_COLOR_NONE = unchecked((int)0xFFFFFFFE);

    // Corner preference
    private const int DWMWCP_DEFAULT = 0;
    private const int DWMWCP_DONOTROUND = 1;
    private const int DWMWCP_ROUND = 2;
    private const int DWMWCP_ROUNDSMALL = 3;

    [DllImport("user32.dll")]
    private static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);

    [DllImport("user32.dll")]
    private static extern int GetWindowLong(IntPtr hwnd, int index);

    [DllImport("user32.dll")]
    private static extern int SetWindowLong(IntPtr hwnd, int index, int newStyle);
    
    [DllImport("user32.dll")]
    private static extern bool SetWindowPos(IntPtr hWnd, IntPtr hWndInsertAfter, int X, int Y, int cx, int cy, uint uFlags);
    
    [DllImport("gdi32.dll")]
    private static extern IntPtr CreateRoundRectRgn(int nLeftRect, int nTopRect, int nRightRect, int nBottomRect, int nWidthEllipse, int nHeightEllipse);
    
    [DllImport("user32.dll")]
    private static extern int SetWindowRgn(IntPtr hWnd, IntPtr hRgn, bool bRedraw);
    
    [DllImport("gdi32.dll")]
    private static extern bool DeleteObject(IntPtr hObject);
    
    [DllImport("dwmapi.dll")]
    private static extern int DwmSetWindowAttribute(IntPtr hwnd, int attr, ref int attrValue, int attrSize);
    
    [StructLayout(LayoutKind.Sequential)]
    private struct MARGINS
    {
        public int cxLeftWidth;
        public int cxRightWidth;
        public int cyTopHeight;
        public int cyBottomHeight;
    }

    [DllImport("dwmapi.dll")]
    private static extern int DwmExtendFrameIntoClientArea(IntPtr hwnd, ref MARGINS pMarInset);
    
}
