using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using silence_.Services;
using System;
using System.Collections.Generic;
using System.Linq;
using System.Runtime.InteropServices;
using Windows.Graphics;

namespace silence_.Pages;

public sealed partial class OverlayPage : Page
{
    private List<ScreenInfo> _screens = new();
    private bool _isInitializing = true;

    public OverlayPage()
    {
        InitializeComponent();
        LoadSettings();
        LoadScreens();
        _isInitializing = false;
    }

    private void LoadSettings()
    {
        var settings = App.Instance?.SettingsService.Settings;
        if (settings == null) return;

        OverlayEnabledToggle.IsOn = settings.OverlayEnabled;
        UpdatePanelsEnabled(settings.OverlayEnabled);

        // Set visibility mode
        foreach (var item in VisibilityModeSelector.Items.Cast<RadioButton>())
        {
            if (item.Tag?.ToString() == settings.OverlayVisibilityMode)
            {
                item.IsChecked = true;
                break;
            }
        }
        
        // Set duration slider
        DurationSlider.Value = settings.OverlayShowDuration;
        DurationLabel.Text = $"Duration: {settings.OverlayShowDuration:F1}s";
        UpdateDurationPanelVisibility(settings.OverlayVisibilityMode);
        
        // Set appearance settings
        ShowTextToggle.IsOn = settings.OverlayShowText;
        
        // Set icon style
        foreach (ComboBoxItem item in IconStyleComboBox.Items)
        {
            if (item.Tag?.ToString() == settings.OverlayIconStyle)
            {
                IconStyleComboBox.SelectedItem = item;
                break;
            }
        }
        
        // Set background style
        foreach (ComboBoxItem item in BackgroundStyleComboBox.Items)
        {
            if (item.Tag?.ToString() == settings.OverlayBackgroundStyle)
            {
                BackgroundStyleComboBox.SelectedItem = item;
                break;
            }
        }
        
        // Set opacity sliders
        OpacitySlider.Value = settings.OverlayOpacity;
        OpacityLabel.Text = $"Background opacity: {settings.OverlayOpacity}%";
        
        ContentOpacitySlider.Value = settings.OverlayContentOpacity;
        ContentOpacityLabel.Text = $"Content opacity: {settings.OverlayContentOpacity}%";
        
        // Set border radius
        BorderRadiusSlider.Value = settings.OverlayBorderRadius;
        BorderRadiusLabel.Text = $"Border radius: {settings.OverlayBorderRadius}px";
        
        // Set show border
        ShowBorderToggle.IsOn = settings.OverlayShowBorder;
        
        // Set scale
        ScaleSlider.Value = settings.OverlayScale;
        ScaleLabel.Text = $"Size scale: {settings.OverlayScale}%";

        UpdatePositionText(settings);
    }

    private void LoadScreens()
    {
        _screens.Clear();
        ScreenComboBox.Items.Clear();

        // Add "Primary Screen" option first
        _screens.Add(new ScreenInfo 
        { 
            DisplayName = "Primary Screen", 
            DeviceName = "PRIMARY",
            IsPrimary = true
        });

        // Get all monitors using Win32 API
        var monitors = GetAllMonitors();
        int screenIndex = 1;
        
        foreach (var monitor in monitors)
        {
            var screenInfo = new ScreenInfo
            {
                DisplayName = $"Screen {screenIndex}: {monitor.DeviceName} ({monitor.WorkArea.Width}x{monitor.WorkArea.Height}){(monitor.IsPrimary ? " - Primary" : "")}",
                DeviceName = monitor.DeviceName,
                WorkArea = monitor.WorkArea,
                IsPrimary = monitor.IsPrimary
            };
            _screens.Add(screenInfo);
            screenIndex++;
        }

        foreach (var screen in _screens)
        {
            ScreenComboBox.Items.Add(screen.DisplayName);
        }

        // Select the saved screen
        var settings = App.Instance?.SettingsService.Settings;
        if (settings != null)
        {
            var savedScreen = settings.OverlayScreenId;
            if (string.IsNullOrEmpty(savedScreen) || savedScreen == "PRIMARY")
            {
                ScreenComboBox.SelectedIndex = 0;
            }
            else
            {
                var index = _screens.FindIndex(s => s.DeviceName == savedScreen);
                ScreenComboBox.SelectedIndex = index >= 0 ? index : 0;
            }
        }
        else
        {
            ScreenComboBox.SelectedIndex = 0;
        }
    }

    private List<MonitorInfo> GetAllMonitors()
    {
        var monitors = new List<MonitorInfo>();
        
        EnumDisplayMonitors(IntPtr.Zero, IntPtr.Zero, (IntPtr hMonitor, IntPtr hdcMonitor, ref RECT lprcMonitor, IntPtr dwData) =>
        {
            var mi = new MONITORINFOEX();
            mi.cbSize = Marshal.SizeOf(mi);
            if (GetMonitorInfo(hMonitor, ref mi))
            {
                monitors.Add(new MonitorInfo
                {
                    Handle = hMonitor,
                    DeviceName = mi.szDevice,
                    WorkArea = new RectInt32(
                        mi.rcWork.Left,
                        mi.rcWork.Top,
                        mi.rcWork.Right - mi.rcWork.Left,
                        mi.rcWork.Bottom - mi.rcWork.Top
                    ),
                    IsPrimary = (mi.dwFlags & MONITORINFOF_PRIMARY) != 0
                });
            }
            return true;
        }, IntPtr.Zero);

        return monitors;
    }
    
    // Win32 API for monitor enumeration
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
    
    private class MonitorInfo
    {
        public IntPtr Handle { get; set; }
        public string DeviceName { get; set; } = "";
        public RectInt32 WorkArea { get; set; }
        public bool IsPrimary { get; set; }
    }

    private void UpdatePositionText(AppSettings settings)
    {
        string horizontalPos;
        string verticalPos;

        // Horizontal position
        if (Math.Abs(settings.OverlayPositionX - 50) < 1)
        {
            horizontalPos = "Center";
        }
        else if (settings.OverlayPositionX < 50)
        {
            horizontalPos = $"Left {settings.OverlayPositionX:F0}%";
        }
        else
        {
            horizontalPos = $"Right {100 - settings.OverlayPositionX:F0}%";
        }

        // Vertical position
        if (Math.Abs(settings.OverlayPositionY - 50) < 1)
        {
            verticalPos = "Middle";
        }
        else if (settings.OverlayPositionY < 50)
        {
            verticalPos = $"Top {settings.OverlayPositionY:F0}%";
        }
        else
        {
            verticalPos = $"Bottom {100 - settings.OverlayPositionY:F0}%";
        }

        CurrentPositionText.Text = $"Current: {horizontalPos}, {verticalPos}";
    }

    private void UpdatePanelsEnabled(bool enabled)
    {
        VisibilityModePanel.Opacity = enabled ? 1.0 : 0.5;
        AppearancePanel.Opacity = enabled ? 1.0 : 0.5;
        ScreenSelectionPanel.Opacity = enabled ? 1.0 : 0.5;
        PositionPanel.Opacity = enabled ? 1.0 : 0.5;
        VisibilityModePanel.IsHitTestVisible = enabled;
        AppearancePanel.IsHitTestVisible = enabled;
        ScreenSelectionPanel.IsHitTestVisible = enabled;
        PositionPanel.IsHitTestVisible = enabled;
    }

    private void OverlayEnabledToggle_Toggled(object sender, RoutedEventArgs e)
    {
        if (_isInitializing) return;

        var enabled = OverlayEnabledToggle.IsOn;
        App.Instance?.SettingsService.UpdateOverlayEnabled(enabled);
        UpdatePanelsEnabled(enabled);
        
        // Show/hide overlay based on setting
        App.Instance?.UpdateOverlayVisibility();
    }

    private void VisibilityModeSelector_SelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        if (_isInitializing) return;

        if (VisibilityModeSelector.SelectedItem is RadioButton selectedButton)
        {
            var mode = selectedButton.Tag?.ToString() ?? "WhenMuted";
            App.Instance?.SettingsService.UpdateOverlayVisibilityMode(mode);
            App.Instance?.UpdateOverlayVisibility();
            UpdateDurationPanelVisibility(mode);
        }
    }
    
    private void UpdateDurationPanelVisibility(string mode)
    {
        DurationPanel.Visibility = mode == "AfterToggle" ? Visibility.Visible : Visibility.Collapsed;
    }
    
    private void DurationSlider_ValueChanged(object sender, Microsoft.UI.Xaml.Controls.Primitives.RangeBaseValueChangedEventArgs e)
    {
        if (_isInitializing) return;
        
        var duration = Math.Round(e.NewValue, 1);
        DurationLabel.Text = $"Duration: {duration:F1}s";
        App.Instance?.SettingsService.UpdateOverlayShowDuration(duration);
    }
    
    private void ShowTextToggle_Toggled(object sender, RoutedEventArgs e)
    {
        if (_isInitializing) return;
        
        App.Instance?.SettingsService.UpdateOverlayShowText(ShowTextToggle.IsOn);
        App.Instance?.ApplyOverlaySettings();
    }
    
    private void IconStyleComboBox_SelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        if (_isInitializing) return;
        
        if (IconStyleComboBox.SelectedItem is ComboBoxItem selectedItem)
        {
            var style = selectedItem.Tag?.ToString() ?? "Colored";
            App.Instance?.SettingsService.UpdateOverlayIconStyle(style);
            App.Instance?.ApplyOverlaySettings();
        }
    }
    
    private void BackgroundStyleComboBox_SelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        if (_isInitializing) return;
        
        if (BackgroundStyleComboBox.SelectedItem is ComboBoxItem selectedItem)
        {
            var style = selectedItem.Tag?.ToString() ?? "Dark";
            App.Instance?.SettingsService.UpdateOverlayBackgroundStyle(style);
            App.Instance?.ApplyOverlaySettings();
        }
    }
    
    private void OpacitySlider_ValueChanged(object sender, Microsoft.UI.Xaml.Controls.Primitives.RangeBaseValueChangedEventArgs e)
    {
        if (_isInitializing) return;
        
        var opacity = (int)e.NewValue;
        OpacityLabel.Text = $"Background opacity: {opacity}%";
        App.Instance?.SettingsService.UpdateOverlayOpacity(opacity);
        App.Instance?.ApplyOverlaySettings();
    }
    
    private void ContentOpacitySlider_ValueChanged(object sender, Microsoft.UI.Xaml.Controls.Primitives.RangeBaseValueChangedEventArgs e)
    {
        if (_isInitializing) return;
        
        var opacity = (int)e.NewValue;
        ContentOpacityLabel.Text = $"Content opacity: {opacity}%";
        App.Instance?.SettingsService.UpdateOverlayContentOpacity(opacity);
        App.Instance?.ApplyOverlaySettings();
    }
    
    private void BorderRadiusSlider_ValueChanged(object sender, Microsoft.UI.Xaml.Controls.Primitives.RangeBaseValueChangedEventArgs e)
    {
        if (_isInitializing) return;
        
        var radius = (int)e.NewValue;
        BorderRadiusLabel.Text = $"Border radius: {radius}px";
        App.Instance?.SettingsService.UpdateOverlayBorderRadius(radius);
        App.Instance?.ApplyOverlaySettings();
    }
    
    private void ShowBorderToggle_Toggled(object sender, RoutedEventArgs e)
    {
        if (_isInitializing) return;
        
        App.Instance?.SettingsService.UpdateOverlayShowBorder(ShowBorderToggle.IsOn);
        App.Instance?.ApplyOverlaySettings();
    }
    
    private void ScaleSlider_ValueChanged(object sender, Microsoft.UI.Xaml.Controls.Primitives.RangeBaseValueChangedEventArgs e)
    {
        if (_isInitializing) return;
        
        var scale = (int)e.NewValue;
        ScaleLabel.Text = $"Size scale: {scale}%";
        App.Instance?.SettingsService.UpdateOverlayScale(scale);
        App.Instance?.ApplyOverlaySettings();
    }

    private void ScreenComboBox_SelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        if (_isInitializing || ScreenComboBox.SelectedIndex < 0) return;

        var selectedScreen = _screens[ScreenComboBox.SelectedIndex];
        App.Instance?.SettingsService.UpdateOverlayScreen(selectedScreen.DeviceName);
        App.Instance?.UpdateOverlayPosition();
    }

    private void SetPositionButton_Click(object sender, RoutedEventArgs e)
    {
        // Enter position editing mode - show overlay and make it draggable
        App.Instance?.StartOverlayPositioning();
        SetPositionButton.Visibility = Visibility.Collapsed;
        DonePositionButton.Visibility = Visibility.Visible;
    }

    private void DonePositionButton_Click(object sender, RoutedEventArgs e)
    {
        // Exit position editing mode
        App.Instance?.StopOverlayPositioning();
        SetPositionButton.Visibility = Visibility.Visible;
        DonePositionButton.Visibility = Visibility.Collapsed;
        
        // Refresh position text
        var settings = App.Instance?.SettingsService.Settings;
        if (settings != null)
        {
            UpdatePositionText(settings);
        }
    }

    private void PreviewOverlayButton_Click(object sender, RoutedEventArgs e)
    {
        // Temporarily show the overlay for preview
        App.Instance?.PreviewOverlay();
    }
    
    public void RefreshPositionText()
    {
        var settings = App.Instance?.SettingsService.Settings;
        if (settings != null)
        {
            UpdatePositionText(settings);
        }
        
        // Also reset button visibility
        SetPositionButton.Visibility = Visibility.Visible;
        DonePositionButton.Visibility = Visibility.Collapsed;
    }
}

public class ScreenInfo
{
    public string DisplayName { get; set; } = "";
    public string DeviceName { get; set; } = "";
    public RectInt32 WorkArea { get; set; }
    public bool IsPrimary { get; set; }
}

