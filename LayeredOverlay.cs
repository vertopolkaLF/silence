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
    
    // Overlay fade animation (show/hide whole window)
    private System.Timers.Timer? _fadeTimer;
    private byte _overlayAlpha = 255;
    private byte _overlayTargetAlpha = 255;
    private const int FadeAnimationDurationMs = 200;
    private const int FadeAnimationIntervalMs = 16; // ~60fps
    private readonly byte _overlayFadeStep = (byte)(255 / (FadeAnimationDurationMs / FadeAnimationIntervalMs));
    
    // Content crossfade animation (icon + text transition) - 2x faster since it's fade out + fade in
    private byte _contentAlpha = 255;
    private byte _contentTargetAlpha = 255;
    private bool _pendingMuteStateChange;
    private bool _pendingMuteState;
    private readonly byte _contentFadeStep = (byte)(255 / (FadeAnimationDurationMs / FadeAnimationIntervalMs) * 2);
    
    // Current state
    private bool _currentMuteState;
    private int _currentX;
    private int _currentY;
    
    // Base dimensions (at 100% DPI)
    private const int BaseIconOnlySize = 48;
    private const int BaseIconFontSize = 28; // Increased icon size
    private const int BaseTextFontSize = 14;
    private const int BasePadding = 10; // Symmetric padding left/right
    private const int BaseIconTextGap = 6; // Gap between icon and text
    
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
    private const string MicrophoneOffGlyphFluent = "\uF781"; // Win11 Segoe Fluent Icons
    private const string MicrophoneOffGlyphMdl2 = "\uE1D6";   // Win10 Segoe MDL2 Assets
    
    // Icon font info
    private string _iconFontFamily = "Segoe Fluent Icons";
    private string _microphoneOffGlyph = MicrophoneOffGlyphFluent;
    
    // Window class name
    private const string ClassName = "SilenceOverlayClass";
    private static bool _classRegistered;
    private static WndProcDelegate? _wndProcDelegate;
    
    public LayeredOverlay()
    {
        RegisterWindowClass();
        CreateOverlayWindow();
        
        // Setup fade animation timer
        _fadeTimer = new System.Timers.Timer(FadeAnimationIntervalMs);
        _fadeTimer.Elapsed += OnFadeTimerTick;
        _fadeTimer.AutoReset = true;
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
        
        // Calculate base DPI scale
        float baseDpiScale = dpi / 96.0f;
        
        // Apply user scale setting (50-200%)
        var settings = App.Instance?.SettingsService.Settings;
        float userScale = settings != null ? settings.OverlayScale / 100.0f : 1.0f;
        
        // Combine DPI scale with user scale
        _dpiScale = baseDpiScale * userScale;
        
        // Recreate fonts with proper scaling
        _iconFont?.Dispose();
        _textFont?.Dispose();
        
        // Try Segoe Fluent Icons first (Win11), fallback to Segoe MDL2 Assets (Win10)
        _iconFontFamily = "Segoe Fluent Icons";
        _microphoneOffGlyph = MicrophoneOffGlyphFluent;
        
        using (var testFont = new Font("Segoe Fluent Icons", 12, FontStyle.Regular, GraphicsUnit.Pixel))
        {
            // If font doesn't exist, GDI+ substitutes another font - check by name
            if (testFont.Name != "Segoe Fluent Icons")
            {
                _iconFontFamily = "Segoe MDL2 Assets";
                _microphoneOffGlyph = MicrophoneOffGlyphMdl2;
            }
        }
        
        _iconFont = new Font(_iconFontFamily, BaseIconFontSize * _dpiScale, FontStyle.Regular, GraphicsUnit.Pixel);
        _textFont = new Font("Segoe UI", BaseTextFontSize * _dpiScale, FontStyle.Regular, GraphicsUnit.Pixel);
        
        // Update dimensions
        _currentWidth = (int)(BaseIconOnlySize * _dpiScale);
        _currentHeight = (int)(BaseIconOnlySize * _dpiScale);
    }
    
    private static IntPtr WndProc(IntPtr hwnd, uint msg, IntPtr wParam, IntPtr lParam)
    {
        const uint WM_DISPLAYCHANGE = 0x007E;
        const uint WM_DPICHANGED = 0x02E0;
        
        if (msg == WM_DISPLAYCHANGE || msg == WM_DPICHANGED)
        {
            // Resolution or DPI changed - need to update overlay position
            // Find the instance that owns this hwnd and trigger repositioning
            App.Instance?.OnDisplayChanged();
        }
        
        return DefWindowProc(hwnd, msg, wParam, lParam);
    }
    
    public void UpdateMuteState(bool isMuted)
    {
        if (_currentMuteState == isMuted && !_pendingMuteStateChange) return;
        
        // If overlay is not visible - set state instantly without animation
        // So when overlay appears it shows fresh content right away
        if (!_isVisible)
        {
            _currentMuteState = isMuted;
            _pendingMuteStateChange = false;
            _contentAlpha = 255;
            _contentTargetAlpha = 255;
            RenderOverlay();
            return;
        }
        
        // Start content crossfade animation
        _pendingMuteState = isMuted;
        _pendingMuteStateChange = true;
        _contentTargetAlpha = 0; // Fade out current content first
        StartFadeTimer();
    }
    
    public void ApplySettings()
    {
        // Recalculate scale (includes user scale setting)
        UpdateDpiScale();
        RenderOverlay();
    }
    
    private void RenderOverlay()
    {
        if (_hwnd == IntPtr.Zero || _iconFont == null || _textFont == null) return;
        
        var settings = App.Instance?.SettingsService.Settings;
        if (settings == null) return;
        
        string variant = settings.OverlayVariant;
        
        // Dot variant has its own simple rendering
        if (variant == "Dot")
        {
            RenderDotVariant();
            return;
        }
        
        // MicIcon variant (original rendering)
        bool showText = settings.OverlayShowText;
        bool isDarkBackground = settings.OverlayBackgroundStyle == "Dark";
        bool isMonochrome = settings.OverlayIconStyle == "Monochrome";
        int bgOpacity = (int)(settings.OverlayOpacity * 255 / 100.0); // Convert 0-100 to 0-255
        int baseContentOpacity = (int)(settings.OverlayContentOpacity * 255 / 100.0); // Convert 0-100 to 0-255
        // Combine settings opacity with animation alpha for content crossfade
        int contentOpacity = baseContentOpacity * _contentAlpha / 255;
        
        // DPI-scaled dimensions
        int scaledIconOnlySize = (int)(BaseIconOnlySize * _dpiScale);
        int scaledPadding = (int)(BasePadding * _dpiScale);
        int scaledGap = (int)(BaseIconTextGap * _dpiScale);
        int scaledCornerRadius = (int)(settings.OverlayBorderRadius * _dpiScale);
        
        // Icon glyph and fixed size (icon fonts are designed for square bounding boxes)
        string glyph = _currentMuteState ? _microphoneOffGlyph : MicrophoneGlyph;
        // Use font size as icon size - icon fonts are designed this way
        int scaledIconSize = (int)Math.Ceiling(BaseIconFontSize * _dpiScale);
        
        // Calculate dimensions
        int oldWidth = _currentWidth;
        string statusText = _currentMuteState ? "Microphone is muted" : "Microphone is unmuted";
        
        if (showText)
        {
            using var measureBmp = new Bitmap(1, 1);
            using var measureG = Graphics.FromImage(measureBmp);
            measureG.TextRenderingHint = TextRenderingHint.ClearTypeGridFit;
            // Use GenericTypographic to get accurate text width without extra padding
            using var measureFormat = new StringFormat(StringFormat.GenericTypographic);
            var textSize = measureG.MeasureString(statusText, _textFont, 1000, measureFormat);
            // Add small buffer to text width (GenericTypographic cuts too tight)
            int textWidth = (int)Math.Ceiling(textSize.Width) + (int)(4 * _dpiScale);
            // Symmetric padding: padding + icon + gap + text + padding
            _currentWidth = scaledPadding + scaledIconSize + scaledGap + textWidth + scaledPadding;
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
        // Use AntiAlias for text when content has transparency or during animation (ClearType doesn't work with alpha)
        g.TextRenderingHint = (contentOpacity < 255 || _contentAlpha < 255)
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
        
        // Win11 style border - subtle, semi-transparent
        if (settings.OverlayShowBorder)
        {
            var borderColor = isDarkBackground 
                ? Color.FromArgb(38, 255, 255, 255)  // White ~15% for dark bg
                : Color.FromArgb(25, 0, 0, 0);       // Black ~10% for light bg
            using var borderPen = new Pen(borderColor, 1);
            DrawRoundedRectangleBorder(g, borderPen, 0, 0, _currentWidth - 1, _currentHeight - 1, scaledCornerRadius);
        }
        
        // Positioning mode border (overrides win11 border)
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
        
        // Draw icon centered in its square area
        // Different icon fonts have different metrics, so we need font-specific horizontal offset
        // Segoe MDL2 Assets (Win10) has fucked up horizontal metrics compared to Segoe Fluent Icons (Win11)
        float horizontalOffset = _iconFontFamily == "Segoe MDL2 Assets" 
            ? 1f * _dpiScale   // Win10 - slight push right
            : 0f;                // Win11 - fine as is
        
        // Original vertical offset to compensate for font baseline bullshit
        float verticalOffset = 2f * _dpiScale;
        
        using (var iconBrush = new SolidBrush(iconColor))
        using (var iconFormat = new StringFormat { Alignment = StringAlignment.Center, LineAlignment = StringAlignment.Center })
        {
            float iconCenterX, iconCenterY;
            
            if (showText)
            {
                // Icon is in a square area at the left side: padding + half icon size
                iconCenterX = scaledPadding + scaledIconSize / 2f + horizontalOffset;
                iconCenterY = _currentHeight / 2f + verticalOffset;
            }
            else
            {
                // Icon centered in the square overlay
                iconCenterX = _currentWidth / 2f + horizontalOffset;
                iconCenterY = _currentHeight / 2f + verticalOffset;
            }
            
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
            float textX = scaledPadding + scaledIconSize + scaledGap;
            float textCenterY = _currentHeight / 2f;
            g.DrawString(statusText, _textFont, textBrush, textX, textCenterY, textFormat);
        }
        
        // Update layered window
        UpdateLayeredWindowBitmap(bitmap, oldWidth != _currentWidth);
    }
    
    private void RenderDotVariant()
    {
        if (_hwnd == IntPtr.Zero) return;
        
        var settings = App.Instance?.SettingsService.Settings;
        if (settings == null) return;
        
        // Dot size based on scale setting
        int baseDotSize = 24; // Base size at 100% scale
        int dotSize = (int)(baseDotSize * _dpiScale);
        
        // Update dimensions if changed
        int oldWidth = _currentWidth;
        _currentWidth = dotSize;
        _currentHeight = dotSize;
        
        // Handle anchor-based repositioning when width changes
        if (oldWidth != _currentWidth && oldWidth > 0)
        {
            int widthDiff = _currentWidth - oldWidth;
            
            if (settings.OverlayPositionX > 60)
            {
                _currentX -= widthDiff;
            }
            else if (settings.OverlayPositionX >= 40)
            {
                _currentX -= widthDiff / 2;
            }
        }
        
        // Create bitmap with alpha channel
        using var bitmap = new Bitmap(_currentWidth, _currentHeight, System.Drawing.Imaging.PixelFormat.Format32bppArgb);
        using var g = Graphics.FromImage(bitmap);
        
        g.SmoothingMode = SmoothingMode.AntiAlias;
        g.Clear(Color.Transparent);
        
        // Dot color based on mute state - simple red/green with content opacity
        int baseContentOpacity = (int)(settings.OverlayContentOpacity * 255 / 100.0);
        int contentOpacity = baseContentOpacity * _contentAlpha / 255;
        
        Color dotColor = _currentMuteState 
            ? Color.FromArgb(contentOpacity, 220, 53, 69)   // Red when muted
            : Color.FromArgb(contentOpacity, 40, 167, 69);  // Green when unmuted
        
        // Draw filled circle - subtract 1 from width/height to prevent clipping
        using (var dotBrush = new SolidBrush(dotColor))
        {
            g.FillEllipse(dotBrush, 0, 0, _currentWidth - 1, _currentHeight - 1);
        }
        
        // Positioning mode border
        if (_isPositioning)
        {
            using var borderPen = new Pen(Color.FromArgb(255, 0, 120, 215), 2 * _dpiScale);
            g.DrawEllipse(borderPen, 1, 1, _currentWidth - 3, _currentHeight - 3);
        }
        
        // Update layered window
        UpdateLayeredWindowBitmap(bitmap, oldWidth != _currentWidth);
    }
    
    private void DrawRoundedRectangle(Graphics g, Brush brush, int x, int y, int width, int height, int radius)
    {
        if (radius <= 0)
        {
            g.FillRectangle(brush, x, y, width, height);
            return;
        }
        
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
        if (radius <= 0)
        {
            g.DrawRectangle(pen, x, y, width, height);
            return;
        }
        
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
                SourceConstantAlpha = _overlayAlpha,
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
        
        _overlayTargetAlpha = 255;
        if (!_isVisible)
        {
            _overlayAlpha = 0;
            ShowWindow(_hwnd, SW_SHOWNOACTIVATE);
            RenderOverlay();
        }
        _isVisible = true;
        StartFadeTimer();
    }
    
    public void HideOverlay()
    {
        if (_hwnd == IntPtr.Zero) return;
        
        _overlayTargetAlpha = 0;
        StartFadeTimer();
    }
    
    private void StartFadeTimer()
    {
        if (_fadeTimer != null && !_fadeTimer.Enabled)
        {
            _fadeTimer.Start();
        }
    }
    
    private void OnFadeTimerTick(object? sender, System.Timers.ElapsedEventArgs e)
    {
        // Animate overlay alpha (window visibility)
        if (_overlayAlpha != _overlayTargetAlpha)
        {
            if (_overlayAlpha < _overlayTargetAlpha)
            {
                int newAlpha = _overlayAlpha + _overlayFadeStep;
                _overlayAlpha = (byte)Math.Min(newAlpha, _overlayTargetAlpha);
            }
            else
            {
                int newAlpha = _overlayAlpha - _overlayFadeStep;
                _overlayAlpha = (byte)Math.Max(newAlpha, _overlayTargetAlpha);
            }
        }
        
        // Animate content alpha (icon + text crossfade) - 2x faster
        if (_contentAlpha != _contentTargetAlpha)
        {
            if (_contentAlpha < _contentTargetAlpha)
            {
                int newAlpha = _contentAlpha + _contentFadeStep;
                _contentAlpha = (byte)Math.Min(newAlpha, _contentTargetAlpha);
            }
            else
            {
                int newAlpha = _contentAlpha - _contentFadeStep;
                _contentAlpha = (byte)Math.Max(newAlpha, _contentTargetAlpha);
            }
        }
        
        // When content faded out completely, apply pending state and fade back in
        if (_pendingMuteStateChange && _contentAlpha == 0)
        {
            _currentMuteState = _pendingMuteState;
            _pendingMuteStateChange = false;
            _contentTargetAlpha = 255; // Fade in new content
        }
        
        // Re-render with current alpha values
        RenderOverlay();
        
        // Check if all animations complete
        bool stillAnimating = _overlayAlpha != _overlayTargetAlpha || 
                              _contentAlpha != _contentTargetAlpha || 
                              _pendingMuteStateChange;
        
        if (!stillAnimating)
        {
            _fadeTimer?.Stop();
            
            if (_overlayTargetAlpha == 0)
            {
                ShowWindow(_hwnd, SW_HIDE);
                _isVisible = false;
            }
        }
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
    
    public void OnDisplayChanged()
    {
        // Update DPI scale for new display configuration
        UpdateDpiScale();
        
        // Recalculate position based on saved percentage values
        var settings = App.Instance?.SettingsService.Settings;
        if (settings != null)
        {
            MoveToPosition(settings.OverlayPositionX, settings.OverlayPositionY, settings.OverlayScreenId);
        }
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
        
        _fadeTimer?.Stop();
        _fadeTimer?.Dispose();
        
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
