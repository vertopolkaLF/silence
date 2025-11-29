using System;
using System.Drawing;
using System.Drawing.Drawing2D;
using System.Drawing.Text;
using System.Runtime.InteropServices;
using silence_.Services;

namespace silence_;

/// <summary>
/// Pure Win32 Layered Window overlay - no WinUI/XAML bullshit.
/// Uses UpdateLayeredWindow for per-pixel alpha transparency.
/// Completely click-through - cursor doesn't even know it exists.
/// </summary>
public sealed class LayeredOverlay : IDisposable
{
    private IntPtr _hwnd;
    private bool _isVisible;
    private bool _isDisposed;
    private bool _isPositioning;
    private bool _isDragging;
    private POINT _dragOffset;
    
    // Current state
    private bool _currentMuteState;
    private int _currentX;
    private int _currentY;
    
    // Base dimensions (at 100% DPI)
    private const int BaseIconOnlySize = 48;
    private const int BaseIconFontSize = 28; // Increased icon size
    private const int BaseTextFontSize = 14;
    private const int BasePadding = 6; // Symmetric padding left/right
    private const int BaseIconTextGap = 2; // Gap between icon and text
    
    // DPI-scaled dimensions
    private float _dpiScale = 1.0f;
    private int _currentWidth;
    private int _currentHeight;
    
    // Magnetic snap
    private const double MagneticRange = 200;
    private const double SnapThreshold = 8;
    
    // Fonts (created with DPI scaling)
    private Font? _iconFont;
    private Font? _textFont;
    
    // Icon glyphs - Segoe Fluent Icons (Win11) or Segoe MDL2 Assets (Win10)
    private const string MicrophoneGlyph = "\uE720";
    private const string MicrophoneOffGlyph = "\uF781";
    
    // Window class name
    private const string ClassName = "SilenceOverlayClass";
    private static bool _classRegistered;
    private static WndProcDelegate? _wndProcDelegate;
    
    public LayeredOverlay()
    {
        RegisterWindowClass();
        CreateOverlayWindow();
    }
    
    private void RegisterWindowClass()
    {
        if (_classRegistered) return;
        
        _wndProcDelegate = WndProc;
        
        var wc = new WNDCLASSEX
        {
            cbSize = Marshal.SizeOf<WNDCLASSEX>(),
            style = 0,
            lpfnWndProc = _wndProcDelegate,
            cbClsExtra = 0,
            cbWndExtra = 0,
            hInstance = GetModuleHandle(null),
            hIcon = IntPtr.Zero,
            hCursor = IntPtr.Zero,
            hbrBackground = IntPtr.Zero,
            lpszMenuName = null,
            lpszClassName = ClassName,
            hIconSm = IntPtr.Zero
        };
        
        if (RegisterClassEx(ref wc) == 0)
        {
            var error = Marshal.GetLastWin32Error();
            if (error != 1410) // ERROR_CLASS_ALREADY_EXISTS
                throw new Exception($"Failed to register window class: {error}");
        }
        
        _classRegistered = true;
    }
    
    private void CreateOverlayWindow()
    {
        // Extended styles: Layered + Transparent (click-through) + ToolWindow (no taskbar) + NoActivate + Topmost
        const int exStyle = WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TOPMOST;
        
        // Style: Popup (no border, no caption)
        const int style = WS_POPUP;
        
        _hwnd = CreateWindowEx(
            exStyle,
            ClassName,
            "silence! overlay",
            style,
            100, 100, // Initial position (will be updated)
            BaseIconOnlySize, BaseIconOnlySize,
            IntPtr.Zero,
            IntPtr.Zero,
            GetModuleHandle(null),
            IntPtr.Zero
        );
        
        if (_hwnd == IntPtr.Zero)
            throw new Exception($"Failed to create window: {Marshal.GetLastWin32Error()}");
        
        // Get DPI for this window
        UpdateDpiScale();
        
        // Initial render
        RenderOverlay();
    }
    
    private void UpdateDpiScale()
    {
        // Get DPI for the window (or primary monitor if window not ready)
        uint dpi = 96;
        
        if (_hwnd != IntPtr.Zero)
        {
            dpi = GetDpiForWindow(_hwnd);
        }
        
        if (dpi == 0) dpi = 96; // Fallback
        
        _dpiScale = dpi / 96.0f;
        
        // Recreate fonts with proper scaling
        _iconFont?.Dispose();
        _textFont?.Dispose();
        
        // Always use Segoe Fluent Icons (available on Win10 1903+ and Win11)
        _iconFont = new Font("Segoe Fluent Icons", BaseIconFontSize * _dpiScale, FontStyle.Regular, GraphicsUnit.Pixel);
        _textFont = new Font("Segoe UI", BaseTextFontSize * _dpiScale, FontStyle.Regular, GraphicsUnit.Pixel);
        
        // Update dimensions
        _currentWidth = (int)(BaseIconOnlySize * _dpiScale);
        _currentHeight = (int)(BaseIconOnlySize * _dpiScale);
    }
    
    private static IntPtr WndProc(IntPtr hwnd, uint msg, IntPtr wParam, IntPtr lParam)
    {
        return DefWindowProc(hwnd, msg, wParam, lParam);
    }
    
    public void UpdateMuteState(bool isMuted)
    {
        _currentMuteState = isMuted;
        RenderOverlay();
    }
    
    public void ApplySettings()
    {
        RenderOverlay();
    }
    
    private void RenderOverlay()
    {
        if (_hwnd == IntPtr.Zero || _iconFont == null || _textFont == null) return;
        
        var settings = App.Instance?.SettingsService.Settings;
        if (settings == null) return;
        
        bool showText = settings.OverlayShowText;
        bool isDarkBackground = settings.OverlayBackgroundStyle == "Dark";
        bool isMonochrome = settings.OverlayIconStyle == "Monochrome";
        int bgOpacity = (int)(settings.OverlayOpacity * 255 / 100.0); // Convert 0-100 to 0-255
        int contentOpacity = (int)(settings.OverlayContentOpacity * 255 / 100.0); // Convert 0-100 to 0-255
        
        // DPI-scaled dimensions
        int scaledIconOnlySize = (int)(BaseIconOnlySize * _dpiScale);
        int scaledPadding = (int)(BasePadding * _dpiScale);
        int scaledGap = (int)(BaseIconTextGap * _dpiScale);
        int scaledCornerRadius = (int)(6 * _dpiScale); // Always use rounded corners
        
        // Measure icon size
        string glyph = _currentMuteState ? MicrophoneOffGlyph : MicrophoneGlyph;
        SizeF iconMeasure;
        using (var measureBmp = new Bitmap(1, 1))
        using (var measureG = Graphics.FromImage(measureBmp))
        {
            iconMeasure = measureG.MeasureString(glyph, _iconFont);
        }
        int scaledIconWidth = (int)Math.Ceiling(iconMeasure.Width);
        
        // Calculate dimensions
        int oldWidth = _currentWidth;
        string statusText = _currentMuteState ? "Microphone is muted" : "Microphone is unmuted";
        
        if (showText)
        {
            using var measureBmp = new Bitmap(1, 1);
            using var measureG = Graphics.FromImage(measureBmp);
            measureG.TextRenderingHint = TextRenderingHint.ClearTypeGridFit;
            var textSize = measureG.MeasureString(statusText, _textFont);
            // Symmetric padding: padding + icon + gap + text + padding
            _currentWidth = scaledPadding + scaledIconWidth + scaledGap + (int)Math.Ceiling(textSize.Width) + scaledPadding;
        }
        else
        {
            _currentWidth = scaledIconOnlySize;
        }
        _currentHeight = scaledIconOnlySize;
        
        // Handle anchor-based repositioning when width changes
        if (oldWidth != _currentWidth && oldWidth > 0)
        {
            int widthDiff = _currentWidth - oldWidth;
            
            // Calculate anchor based on position percentage
            // < 40% = left anchor, > 60% = right anchor, 40-60% = center anchor
            if (settings.OverlayPositionX > 60)
            {
                // Right anchor: shift left
                _currentX -= widthDiff;
            }
            else if (settings.OverlayPositionX >= 40)
            {
                // Center anchor: shift half
                _currentX -= widthDiff / 2;
            }
            // Left anchor: no shift needed
        }
        
        // Create bitmap with alpha channel
        using var bitmap = new Bitmap(_currentWidth, _currentHeight, System.Drawing.Imaging.PixelFormat.Format32bppArgb);
        using var g = Graphics.FromImage(bitmap);
        
        g.SmoothingMode = SmoothingMode.AntiAlias;
        // Use AntiAlias for text when content has transparency (ClearType doesn't work with alpha)
        g.TextRenderingHint = contentOpacity < 255 
            ? TextRenderingHint.AntiAliasGridFit 
            : TextRenderingHint.ClearTypeGridFit;
        g.Clear(Color.Transparent);
        
        // Background color with configurable opacity
        var bgColor = isDarkBackground 
            ? Color.FromArgb(bgOpacity, 30, 30, 30) 
            : Color.FromArgb(bgOpacity, 255, 255, 255);
        
        // Draw rounded rectangle background
        using (var bgBrush = new SolidBrush(bgColor))
        {
            DrawRoundedRectangle(g, bgBrush, 0, 0, _currentWidth, _currentHeight, scaledCornerRadius);
        }
        
        // Positioning mode border
        if (_isPositioning)
        {
            using var borderPen = new Pen(Color.FromArgb(255, 0, 120, 215), 2 * _dpiScale);
            DrawRoundedRectangleBorder(g, borderPen, 1, 1, _currentWidth - 2, _currentHeight - 2, scaledCornerRadius);
        }
        
        // Icon color with content opacity
        Color iconColor;
        if (isMonochrome)
        {
            iconColor = isDarkBackground 
                ? Color.FromArgb(contentOpacity, 255, 255, 255)  // White
                : Color.FromArgb(contentOpacity, 0, 0, 0);       // Black
        }
        else
        {
            iconColor = _currentMuteState 
                ? Color.FromArgb(contentOpacity, 220, 53, 69)   // Red
                : Color.FromArgb(contentOpacity, 40, 167, 69);  // Green
        }
        
        // Draw icon - with manual vertical correction (icon fonts have fucked up baseline)
        using (var iconBrush = new SolidBrush(iconColor))
        using (var iconFormat = new StringFormat { Alignment = StringAlignment.Center, LineAlignment = StringAlignment.Center })
        {
            float iconCenterX = showText 
                ? scaledPadding + scaledIconWidth / 2f  // Left aligned with padding
                : _currentWidth / 2f;  // Centered when icon only
            // Shift icon up by ~10% of font size to compensate for font metrics
            float iconVerticalOffset = 2f * _dpiScale;
            float iconCenterY = _currentHeight / 2f + iconVerticalOffset;
            g.DrawString(glyph, _iconFont, iconBrush, iconCenterX, iconCenterY, iconFormat);
        }
        
        // Draw text if enabled - also use StringFormat for consistent vertical alignment
        if (showText)
        {
            var textColor = isDarkBackground 
                ? Color.FromArgb(contentOpacity, 255, 255, 255)  // White
                : Color.FromArgb(contentOpacity, 0, 0, 0);       // Black
            using var textBrush = new SolidBrush(textColor);
            using var textFormat = new StringFormat { Alignment = StringAlignment.Near, LineAlignment = StringAlignment.Center };
            float textX = scaledPadding + scaledIconWidth + scaledGap;
            float textCenterY = _currentHeight / 2f;
            g.DrawString(statusText, _textFont, textBrush, textX, textCenterY, textFormat);
        }
        
        // Update layered window
        UpdateLayeredWindowBitmap(bitmap, oldWidth != _currentWidth);
    }
    
    private void DrawRoundedRectangle(Graphics g, Brush brush, int x, int y, int width, int height, int radius)
    {
        using var path = new GraphicsPath();
        path.AddArc(x, y, radius * 2, radius * 2, 180, 90);
        path.AddArc(x + width - radius * 2, y, radius * 2, radius * 2, 270, 90);
        path.AddArc(x + width - radius * 2, y + height - radius * 2, radius * 2, radius * 2, 0, 90);
        path.AddArc(x, y + height - radius * 2, radius * 2, radius * 2, 90, 90);
        path.CloseFigure();
        g.FillPath(brush, path);
    }
    
    private void DrawRoundedRectangleBorder(Graphics g, Pen pen, int x, int y, int width, int height, int radius)
    {
        using var path = new GraphicsPath();
        path.AddArc(x, y, radius * 2, radius * 2, 180, 90);
        path.AddArc(x + width - radius * 2, y, radius * 2, radius * 2, 270, 90);
        path.AddArc(x + width - radius * 2, y + height - radius * 2, radius * 2, radius * 2, 0, 90);
        path.AddArc(x, y + height - radius * 2, radius * 2, radius * 2, 90, 90);
        path.CloseFigure();
        g.DrawPath(pen, path);
    }
    
    private void UpdateLayeredWindowBitmap(Bitmap bitmap, bool sizeChanged)
    {
        IntPtr screenDc = GetDC(IntPtr.Zero);
        IntPtr memDc = CreateCompatibleDC(screenDc);
        IntPtr hBitmap = bitmap.GetHbitmap(Color.FromArgb(0));
        IntPtr oldBitmap = SelectObject(memDc, hBitmap);
        
        try
        {
            var size = new SIZE { cx = bitmap.Width, cy = bitmap.Height };
            var sourcePoint = new POINT { X = 0, Y = 0 };
            var windowPoint = new POINT { X = _currentX, Y = _currentY };
            var blend = new BLENDFUNCTION
            {
                BlendOp = AC_SRC_OVER,
                BlendFlags = 0,
                SourceConstantAlpha = 255,
                AlphaFormat = AC_SRC_ALPHA
            };
            
            UpdateLayeredWindow(_hwnd, screenDc, ref windowPoint, ref size, memDc, ref sourcePoint, 0, ref blend, ULW_ALPHA);
        }
        finally
        {
            SelectObject(memDc, oldBitmap);
            DeleteObject(hBitmap);
            DeleteDC(memDc);
            ReleaseDC(IntPtr.Zero, screenDc);
        }
    }
    
    public void ShowOverlay()
    {
        if (_hwnd == IntPtr.Zero) return;
        ShowWindow(_hwnd, SW_SHOWNOACTIVATE);
        _isVisible = true;
    }
    
    public void HideOverlay()
    {
        if (_hwnd == IntPtr.Zero) return;
        ShowWindow(_hwnd, SW_HIDE);
        _isVisible = false;
    }
    
    public void MoveToPosition(double percentX, double percentY, string screenId)
    {
        var workArea = GetTargetScreenWorkArea(screenId);
        
        // Ensure dimensions are valid
        if (_currentWidth == 0 || _currentHeight == 0)
        {
            _currentWidth = (int)(BaseIconOnlySize * _dpiScale);
            _currentHeight = (int)(BaseIconOnlySize * _dpiScale);
        }
        
        _currentX = (int)(workArea.X + (workArea.Width - _currentWidth) * percentX / 100.0);
        _currentY = (int)(workArea.Y + (workArea.Height - _currentHeight) * percentY / 100.0);
        
        // Update window position - for layered windows we need to call UpdateLayeredWindow
        if (_hwnd != IntPtr.Zero)
        {
            RenderOverlay(); // This will update position via UpdateLayeredWindow
        }
    }
    
    public void StartPositioning()
    {
        _isPositioning = true;
        SetClickThrough(false);
        RenderOverlay();
        ShowOverlay();
    }
    
    public void StopPositioning()
    {
        _isPositioning = false;
        _isDragging = false;
        SetClickThrough(true);
        RenderOverlay();
        SaveCurrentPosition();
    }
    
    private void SetClickThrough(bool clickThrough)
    {
        if (_hwnd == IntPtr.Zero) return;
        
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
    
    private void SaveCurrentPosition()
    {
        var settings = App.Instance?.SettingsService.Settings;
        if (settings == null) return;
        
        var workArea = GetTargetScreenWorkArea(settings.OverlayScreenId);
        
        double percentX = (_currentX - workArea.X) * 100.0 / (workArea.Width - _currentWidth);
        double percentY = (_currentY - workArea.Y) * 100.0 / (workArea.Height - _currentHeight);
        
        percentX = Math.Clamp(percentX, 0, 100);
        percentY = Math.Clamp(percentY, 0, 100);
        
        App.Instance?.SettingsService.UpdateOverlayPosition(percentX, percentY);
    }
    
    private RECT GetTargetScreenWorkArea(string screenId)
    {
        RECT result = new RECT { Left = 0, Top = 0, Right = 1920, Bottom = 1080 };
        
        EnumDisplayMonitors(IntPtr.Zero, IntPtr.Zero, (IntPtr hMonitor, IntPtr hdcMonitor, ref RECT lprcMonitor, IntPtr dwData) =>
        {
            var mi = new MONITORINFOEX();
            mi.cbSize = Marshal.SizeOf(mi);
            
            if (GetMonitorInfo(hMonitor, ref mi))
            {
                bool isTarget = false;
                
                if (screenId == "PRIMARY" || string.IsNullOrEmpty(screenId))
                {
                    isTarget = (mi.dwFlags & MONITORINFOF_PRIMARY) != 0;
                }
                else
                {
                    isTarget = mi.szDevice == screenId;
                }
                
                if (isTarget)
                {
                    result = mi.rcWork;
                    return false;
                }
            }
            return true;
        }, IntPtr.Zero);
        
        return result;
    }
    
    public void ProcessDrag()
    {
        if (!_isPositioning) return;
        
        // Check for Escape key to exit positioning mode
        if ((GetAsyncKeyState(VK_ESCAPE) & 0x8000) != 0)
        {
            App.Instance?.StopOverlayPositioning();
            return;
        }
        
        GetCursorPos(out POINT cursorPos);
        
        if (!_isDragging)
        {
            // Check if mouse button is down
            if ((GetAsyncKeyState(VK_LBUTTON) & 0x8000) != 0)
            {
                // Check if cursor is over our window
                if (cursorPos.X >= _currentX && cursorPos.X < _currentX + _currentWidth &&
                    cursorPos.Y >= _currentY && cursorPos.Y < _currentY + _currentHeight)
                {
                    _isDragging = true;
                    _dragOffset.X = cursorPos.X - _currentX;
                    _dragOffset.Y = cursorPos.Y - _currentY;
                }
            }
        }
        else
        {
            if ((GetAsyncKeyState(VK_LBUTTON) & 0x8000) == 0)
            {
                // Mouse released
                _isDragging = false;
            }
            else
            {
                // Dragging - calculate new position with magnetic snap
                var settings = App.Instance?.SettingsService.Settings;
                var workArea = GetTargetScreenWorkArea(settings?.OverlayScreenId ?? "PRIMARY");
                
                double baseX = cursorPos.X - _dragOffset.X;
                double baseY = cursorPos.Y - _dragOffset.Y;
                
                double centerX = workArea.Left + (workArea.Right - workArea.Left - _currentWidth) / 2.0;
                double centerY = workArea.Top + (workArea.Bottom - workArea.Top - _currentHeight) / 2.0;
                
                double distanceFromCenterX = Math.Abs(baseX - centerX);
                double distanceFromCenterY = Math.Abs(baseY - centerY);
                
                double finalX = baseX;
                double finalY = baseY;
                
                // Magnetic snap to center X
                if (distanceFromCenterX < MagneticRange)
                {
                    if (distanceFromCenterX < SnapThreshold)
                        finalX = centerX;
                    else
                    {
                        double t = 1.0 - (distanceFromCenterX / MagneticRange);
                        double strength = t * t * t;
                        finalX = baseX + (centerX - baseX) * strength;
                    }
                }
                
                // Magnetic snap to center Y
                if (distanceFromCenterY < MagneticRange)
                {
                    if (distanceFromCenterY < SnapThreshold)
                        finalY = centerY;
                    else
                    {
                        double t = 1.0 - (distanceFromCenterY / MagneticRange);
                        double strength = t * t * t;
                        finalY = baseY + (centerY - baseY) * strength;
                    }
                }
                
                _currentX = (int)Math.Round(finalX);
                _currentY = (int)Math.Round(finalY);
                
                RenderOverlay();
            }
        }
    }
    
    public void Close()
    {
        Dispose();
    }
    
    public void Dispose()
    {
        if (_isDisposed) return;
        _isDisposed = true;
        
        _iconFont?.Dispose();
        _textFont?.Dispose();
        
        if (_hwnd != IntPtr.Zero)
        {
            DestroyWindow(_hwnd);
            _hwnd = IntPtr.Zero;
        }
        
        GC.SuppressFinalize(this);
    }
    
    ~LayeredOverlay()
    {
        Dispose();
    }
    
    #region Win32 API
    
    private delegate IntPtr WndProcDelegate(IntPtr hwnd, uint msg, IntPtr wParam, IntPtr lParam);
    private delegate bool MonitorEnumProc(IntPtr hMonitor, IntPtr hdcMonitor, ref RECT lprcMonitor, IntPtr dwData);
    
    private const int GWL_EXSTYLE = -20;
    private const int WS_EX_LAYERED = 0x00080000;
    private const int WS_EX_TRANSPARENT = 0x00000020;
    private const int WS_EX_TOOLWINDOW = 0x00000080;
    private const int WS_EX_NOACTIVATE = 0x08000000;
    private const int WS_EX_TOPMOST = 0x00000008;
    private const int WS_POPUP = unchecked((int)0x80000000);
    
    private const int SW_HIDE = 0;
    private const int SW_SHOWNOACTIVATE = 4;
    
    private const byte AC_SRC_OVER = 0x00;
    private const byte AC_SRC_ALPHA = 0x01;
    private const int ULW_ALPHA = 0x00000002;
    
    private const int MONITORINFOF_PRIMARY = 0x00000001;
    private const int VK_LBUTTON = 0x01;
    private const int VK_ESCAPE = 0x1B;
    
    [StructLayout(LayoutKind.Sequential)]
    private struct POINT
    {
        public int X;
        public int Y;
    }
    
    [StructLayout(LayoutKind.Sequential)]
    private struct SIZE
    {
        public int cx;
        public int cy;
    }
    
    [StructLayout(LayoutKind.Sequential)]
    private struct RECT
    {
        public int Left;
        public int Top;
        public int Right;
        public int Bottom;
        
        public int Width => Right - Left;
        public int Height => Bottom - Top;
        public int X => Left;
        public int Y => Top;
    }
    
    [StructLayout(LayoutKind.Sequential)]
    private struct BLENDFUNCTION
    {
        public byte BlendOp;
        public byte BlendFlags;
        public byte SourceConstantAlpha;
        public byte AlphaFormat;
    }
    
    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Auto)]
    private struct WNDCLASSEX
    {
        public int cbSize;
        public int style;
        public WndProcDelegate lpfnWndProc;
        public int cbClsExtra;
        public int cbWndExtra;
        public IntPtr hInstance;
        public IntPtr hIcon;
        public IntPtr hCursor;
        public IntPtr hbrBackground;
        public string? lpszMenuName;
        public string lpszClassName;
        public IntPtr hIconSm;
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
    
    [DllImport("user32.dll", SetLastError = true, CharSet = CharSet.Auto)]
    private static extern ushort RegisterClassEx(ref WNDCLASSEX lpwcx);
    
    [DllImport("user32.dll", SetLastError = true, CharSet = CharSet.Auto)]
    private static extern IntPtr CreateWindowEx(int dwExStyle, string lpClassName, string lpWindowName, int dwStyle, int x, int y, int nWidth, int nHeight, IntPtr hWndParent, IntPtr hMenu, IntPtr hInstance, IntPtr lpParam);
    
    [DllImport("user32.dll")]
    private static extern IntPtr DefWindowProc(IntPtr hWnd, uint uMsg, IntPtr wParam, IntPtr lParam);
    
    [DllImport("user32.dll")]
    private static extern bool DestroyWindow(IntPtr hWnd);
    
    [DllImport("user32.dll")]
    private static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);
    
    [DllImport("user32.dll")]
    private static extern int GetWindowLong(IntPtr hWnd, int nIndex);
    
    [DllImport("user32.dll")]
    private static extern int SetWindowLong(IntPtr hWnd, int nIndex, int dwNewLong);
    
    [DllImport("user32.dll")]
    private static extern bool UpdateLayeredWindow(IntPtr hwnd, IntPtr hdcDst, ref POINT pptDst, ref SIZE psize, IntPtr hdcSrc, ref POINT pptSrc, int crKey, ref BLENDFUNCTION pblend, int dwFlags);
    
    [DllImport("user32.dll")]
    private static extern IntPtr GetDC(IntPtr hWnd);
    
    [DllImport("user32.dll")]
    private static extern int ReleaseDC(IntPtr hWnd, IntPtr hDC);
    
    [DllImport("gdi32.dll")]
    private static extern IntPtr CreateCompatibleDC(IntPtr hdc);
    
    [DllImport("gdi32.dll")]
    private static extern bool DeleteDC(IntPtr hdc);
    
    [DllImport("gdi32.dll")]
    private static extern IntPtr SelectObject(IntPtr hdc, IntPtr hgdiobj);
    
    [DllImport("gdi32.dll")]
    private static extern bool DeleteObject(IntPtr hObject);
    
    [DllImport("kernel32.dll", CharSet = CharSet.Auto)]
    private static extern IntPtr GetModuleHandle(string? lpModuleName);
    
    [DllImport("user32.dll")]
    private static extern bool EnumDisplayMonitors(IntPtr hdc, IntPtr lprcClip, MonitorEnumProc lpfnEnum, IntPtr dwData);
    
    [DllImport("user32.dll", CharSet = CharSet.Auto)]
    private static extern bool GetMonitorInfo(IntPtr hMonitor, ref MONITORINFOEX lpmi);
    
    [DllImport("user32.dll")]
    private static extern bool GetCursorPos(out POINT lpPoint);
    
    [DllImport("user32.dll")]
    private static extern short GetAsyncKeyState(int vKey);
    
    [DllImport("user32.dll")]
    private static extern uint GetDpiForWindow(IntPtr hwnd);
    
    #endregion
}
