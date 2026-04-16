using H.NotifyIcon;
using Microsoft.UI;
using Microsoft.UI.Composition.SystemBackdrops;
using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Media.Animation;
using silence_.Pages;
using silence_.Services;
using System;
using System.Drawing;
using System.Drawing.Drawing2D;
using System.Runtime.InteropServices;
using Windows.Graphics;
using WinRT.Interop;

namespace silence_
{
    public sealed partial class MainWindow : Window
    {
        private TaskbarIcon? _trayIcon;
        private AppWindow? _appWindow;
        private const int BaseMinWindowWidth = 580;
        private const int BaseMinWindowHeight = 480;

        private string _currentPage = "General";
        private bool _updateAvailable = false;
        private MenuFlyout? _trayMenu;
        private MenuFlyoutItem? _appInfoItem;
        private MenuFlyoutItem? _showItem;
        private MenuFlyoutItem? _muteItem;
        private MenuFlyoutItem? _refreshOverlayItem;
        private MenuFlyoutItem? _exitItem;

        public void NavigateToAbout()
        {
            NavView.SelectedItem = AboutNavItem;
            _updateAvailable = false;
            UpdateNotificationBorder.Visibility = Visibility.Collapsed;
            UpdateNotificationCompact.Visibility = Visibility.Collapsed;
            UpdatePlaceholder.Visibility = Visibility.Visible;
        }

        private void SetupTitleBar()
        {
            ExtendsContentIntoTitleBar = true;
            SetTitleBar(AppTitleBar);
        }

        private void UpdateTitleBarColors()
        {
            if (_appWindow?.TitleBar == null) return;
            
            var titleBar = _appWindow.TitleBar;
            // Keep backgrounds transparent
            titleBar.ButtonBackgroundColor = Colors.Transparent;
            titleBar.ButtonInactiveBackgroundColor = Colors.Transparent;
            titleBar.ButtonInactiveForegroundColor = Windows.UI.Color.FromArgb(10, 100, 100, 100);
            
            // Reset to default colors to allow system to show disabled state properly
            titleBar.ButtonForegroundColor = null;
            titleBar.ButtonHoverForegroundColor = null;
            titleBar.ButtonHoverBackgroundColor = null;
            titleBar.ButtonPressedBackgroundColor = null;
            titleBar.ButtonPressedForegroundColor = null;
        }

        private void SetupBackdrop()
        {
            var version = Environment.OSVersion.Version;
            bool isWin11 = version.Build >= 22000;

            if (isWin11 && MicaController.IsSupported())
            {
                SystemBackdrop = new MicaBackdrop();
            }
            else if (DesktopAcrylicController.IsSupported())
            {
                SystemBackdrop = new DesktopAcrylicBackdrop();
            }
        }

        public MainWindow()
        {
            InitializeComponent();
            
            SetupTitleBar();
            SetupBackdrop();
            SetupWindow();
            SetupTrayIcon();
            
            UpdateTitleBarColors();
            ApplyLocalizedStrings();
            
            if (Content is FrameworkElement rootElement)
            {
                rootElement.ActualThemeChanged += (s, e) => UpdateTitleBarColors();
            }

            // Navigate to General page initially
            ContentFrame.Navigate(typeof(GeneralPage), null, new SuppressNavigationTransitionInfo());

            this.Closed += MainWindow_Closed;
            
            // Subscribe to mute state changes for tray icon
            if (App.Instance != null)
            {
                App.Instance.MuteStateChanged += OnMuteStateChanged;
                App.Instance.UpdateAvailable += OnUpdateAvailable;
                App.Instance.LocalizationService.LanguageChanged += OnLanguageChanged;
            }
            UpdateTrayIcon(App.Instance?.MicrophoneService.IsMuted() ?? false);
        }

        private void OnLanguageChanged()
        {
            DispatcherQueue.TryEnqueue(RefreshLocalizedUI);
        }
        
        private void OnUpdateAvailable(UpdateCheckResult result)
        {
            DispatcherQueue.TryEnqueue(() =>
            {
                _updateAvailable = true;
                UpdatePlaceholder.Visibility = Visibility.Collapsed;
                
                // Show appropriate notification based on pane state
                if (NavView.IsPaneOpen)
                {
                    UpdateNotificationBorder.Visibility = Visibility.Visible;
                    UpdateNotificationCompact.Visibility = Visibility.Collapsed;
                }
                else
                {
                    UpdateNotificationBorder.Visibility = Visibility.Collapsed;
                    UpdateNotificationCompact.Visibility = Visibility.Visible;
                }
            });
        }
        
        private void UpdateInfoBar_ActionClick(object sender, RoutedEventArgs e)
        {
            NavigateToAboutAndHideNotification();
        }
        
        private void UpdateNotificationCompact_Click(object sender, Microsoft.UI.Xaml.Input.PointerRoutedEventArgs e)
        {
            NavigateToAboutAndHideNotification();
        }
        
        private void UpdateNotificationCompact_PointerEntered(object sender, Microsoft.UI.Xaml.Input.PointerRoutedEventArgs e)
        {
            // Use the same hover color as NavigationViewItem
            if (Application.Current.Resources.TryGetValue("NavigationViewItemBackgroundPointerOver", out var brush))
            {
                UpdateNotificationCompact.Background = brush as Microsoft.UI.Xaml.Media.Brush;
            }
            else
            {
                // Fallback - matches NavigationView hover
                UpdateNotificationCompact.Background = new SolidColorBrush(
                    (Content as FrameworkElement)?.ActualTheme == ElementTheme.Dark
                        ? Windows.UI.Color.FromArgb(15, 255, 255, 255)
                        : Windows.UI.Color.FromArgb(15, 0, 0, 0));
            }
        }
        
        private void UpdateNotificationCompact_PointerExited(object sender, Microsoft.UI.Xaml.Input.PointerRoutedEventArgs e)
        {
            UpdateNotificationCompact.Background = new SolidColorBrush(Colors.Transparent);
        }
        
        private void NavigateToAboutAndHideNotification()
        {
            // Navigate to About page to show update details
            NavView.SelectedItem = AboutNavItem;
            _updateAvailable = false;
            UpdateNotificationBorder.Visibility = Visibility.Collapsed;
            UpdateNotificationCompact.Visibility = Visibility.Collapsed;
            UpdatePlaceholder.Visibility = Visibility.Visible;
        }

        private void OnMuteStateChanged(bool isMuted)
        {
            DispatcherQueue.TryEnqueue(() => UpdateTrayIcon(isMuted));
        }

        private void SetupWindow()
        {
            var hwnd = WindowNative.GetWindowHandle(this);
            var windowId = Win32Interop.GetWindowIdFromWindow(hwnd);
            _appWindow = AppWindow.GetFromWindowId(windowId);

            if (_appWindow != null)
            {
                // Disable maximize button using Win32 API
                DisableMaximizeButton(hwnd);


                // Get DPI scaling factor
                var dpi = GetDpiForWindow(hwnd);
                _currentDpiScale = dpi / 96.0; // 96 DPI = 100% scaling
                
                // Adjust window size based on DPI scaling to maintain consistent visual size
                var adjustedWidth = (int)(BaseMinWindowWidth * _currentDpiScale);
                var adjustedHeight = (int)(BaseMinWindowHeight * _currentDpiScale);
                
                _appWindow.Resize(new SizeInt32(adjustedWidth, adjustedHeight));
                
                _appWindow.Changed += (s, e) =>
                {
                    if (e.DidSizeChange)
                    {
                        EnforceMinimumWindowSize();
                    }
                };
                
                var displayArea = DisplayArea.GetFromWindowId(windowId, DisplayAreaFallback.Primary);
                if (displayArea != null)
                {
                    var windowWidth = (int)(BaseMinWindowWidth * _currentDpiScale);
                    var windowHeight = (int)(BaseMinWindowHeight * _currentDpiScale);
                    var centerX = (displayArea.WorkArea.Width - windowWidth) / 2;
                    var centerY = (displayArea.WorkArea.Height - windowHeight) / 2;
                    _appWindow.Move(new PointInt32(centerX, centerY));
                }

                _appWindow.Title = AppResources.GetString("App.Title");
                SetWindowIcon();
            }
        }

        private void EnforceMinimumWindowSize()
        {
            if (_appWindow == null) return;
            
            var minWidth = (int)(BaseMinWindowWidth * _currentDpiScale);
            var minHeight = (int)(BaseMinWindowHeight * _currentDpiScale);
            
            if (_appWindow.Size.Width < minWidth)
            {
                _appWindow.Resize(new SizeInt32(minWidth, _appWindow.Size.Height));
            }
            if (_appWindow.Size.Height < minHeight)
            {
                _appWindow.Resize(new SizeInt32(_appWindow.Size.Width, minHeight));
            }
        }

        private void UpdateMinimumWindowSize()
        {
            if (_appWindow == null) return;
            
            // Calculate new minimum dimensions based on updated DPI scale
            var minWidth = (int)(BaseMinWindowWidth * _currentDpiScale);
            var minHeight = (int)(BaseMinWindowHeight * _currentDpiScale);
            
            // If current size is smaller than new minimum, resize to minimum
            bool needsResize = false;
            int newWidth = _appWindow.Size.Width;
            int newHeight = _appWindow.Size.Height;
            
            if (newWidth < minWidth)
            {
                newWidth = minWidth;
                needsResize = true;
            }
            if (newHeight < minHeight)
            {
                newHeight = minHeight;
                needsResize = true;
            }
            
            if (needsResize)
            {
                _appWindow.Resize(new SizeInt32(newWidth, newHeight));
            }
        }

        private void SetWindowIcon()
        {
            if (_appWindow == null) return;

            try
            {
                var baseDir = AppContext.BaseDirectory;
                var iconPath = System.IO.Path.Combine(baseDir, "Assets", "app.ico");
                
                if (System.IO.File.Exists(iconPath))
                {
                    _appWindow.SetIcon(iconPath);
                    return;
                }

                iconPath = System.IO.Path.Combine(baseDir, "Assets", "app.png");
                if (System.IO.File.Exists(iconPath))
                {
                    _appWindow.SetIcon(iconPath);
                }
            }
            catch (Exception ex)
            {
                System.Diagnostics.Debug.WriteLine($"Failed to set window icon: {ex.Message}");
            }
        }

        private void SetupTrayIcon()
        {
            _trayIcon = new TaskbarIcon
            {
                NoLeftClickDelay = true
            };
            
            _trayMenu = new MenuFlyout();
            
            // App name and version (disabled)
            _appInfoItem = new MenuFlyoutItem
            { 
                Text = string.Empty,
                IsEnabled = false
            };
            _trayMenu.Items.Add(_appInfoItem);

            _trayMenu.Items.Add(new MenuFlyoutSeparator());
            
            _showItem = new MenuFlyoutItem
            { 
                Text = string.Empty,
                Command = new RelayCommand(ShowWindow)
            };
            _trayMenu.Items.Add(_showItem);

            _muteItem = new MenuFlyoutItem
            { 
                Text = string.Empty,
                Command = new RelayCommand(() => App.Instance?.ToggleMute())
            };
            _trayMenu.Items.Add(_muteItem);

            _refreshOverlayItem = new MenuFlyoutItem
            {
                Text = string.Empty,
                Command = new RelayCommand(() => App.Instance?.RefreshOverlay())
            };
            _trayMenu.Items.Add(_refreshOverlayItem);

            _trayMenu.Items.Add(new MenuFlyoutSeparator());

            _exitItem = new MenuFlyoutItem
            { 
                Text = string.Empty,
                Command = new RelayCommand(() =>
                {
                    _trayIcon?.Dispose();
                    App.Instance?.ExitApplication();
                })
            };
            _trayMenu.Items.Add(_exitItem);

            _trayIcon.ContextFlyout = _trayMenu;
            _trayIcon.LeftClickCommand = new RelayCommand(() => App.Instance?.ToggleMute());
            _trayIcon.ToolTipText = string.Empty;

            UpdateTrayIcon(false);
            ApplyLocalizedStrings();
            _trayIcon.ForceCreate();
        }

        private void UpdateTrayIcon(bool isMuted)
        {
            if (_trayIcon == null) return;

            try
            {
                var style = App.Instance?.SettingsService.Settings.TrayIconStyle ?? "Standard";
                var icon = CreateMicrophoneIcon(isMuted, style);
                _trayIcon.Icon = icon;
                _trayIcon.ToolTipText = isMuted
                    ? AppResources.GetString("Tray.Tooltip.Muted")
                    : AppResources.GetString("Tray.Tooltip.Unmuted");
            }
            catch
            {
                // Icon creation failed
            }
        }

        public void RefreshTrayIcon()
        {
            UpdateTrayIcon(App.Instance?.MicrophoneService.IsMuted() ?? false);
        }

        public void RefreshLocalizedUI()
        {
            ApplyLocalizedStrings();
            NavigateToPage(_currentPage, new SuppressNavigationTransitionInfo(), forceReload: true);
            UpdateTrayIcon(App.Instance?.MicrophoneService.IsMuted() ?? false);
        }

        private static Icon CreateMicrophoneIcon(bool isMuted, string style)
        {
            const int size = 32;
            using var bitmap = new Bitmap(size, size);
            using var g = Graphics.FromImage(bitmap);

            g.SmoothingMode = SmoothingMode.AntiAlias;
            g.PixelOffsetMode = PixelOffsetMode.HighQuality;
            g.Clear(System.Drawing.Color.Transparent);

            var stateColor = isMuted
                ? System.Drawing.Color.FromArgb(220, 53, 69)
                : System.Drawing.Color.FromArgb(40, 167, 69);

            if (string.Equals(style, "FilledCircle", StringComparison.OrdinalIgnoreCase))
            {
                DrawFilledCircleTrayIcon(g, size, stateColor);
            }
            else if (string.Equals(style, "Dot", StringComparison.OrdinalIgnoreCase))
            {
                DrawDotTrayIcon(g, size, stateColor);
            }
            else
            {
                DrawStandardTrayIcon(g, stateColor);
            }

            var iconHandle = bitmap.GetHicon();
            try
            {
                using var icon = Icon.FromHandle(iconHandle);
                return (Icon)icon.Clone();
            }
            finally
            {
                DestroyIcon(iconHandle);
            }
        }

        private static void DrawStandardTrayIcon(Graphics g, System.Drawing.Color color)
        {
            using var brush = new SolidBrush(color);
            using var pen = new Pen(color, 2.6f)
            {
                StartCap = LineCap.Round,
                EndCap = LineCap.Round
            };

            using var bodyPath = CreateRoundedRectanglePath(new RectangleF(10.5f, 4f, 11f, 14f), 5.5f);
            g.FillPath(brush, bodyPath);

            g.DrawArc(pen, 6f, 11.5f, 20f, 13f, 0, 180);
            g.DrawLine(pen, 16f, 24.5f, 16f, 27.5f);
            g.DrawLine(pen, 11f, 27.5f, 21f, 27.5f);
        }

        private static void DrawFilledCircleTrayIcon(Graphics g, int size, System.Drawing.Color color)
        {
            using var circleBrush = new SolidBrush(color);
            g.FillEllipse(circleBrush, 1, 1, size - 2, size - 2);

            using var micBrush = new SolidBrush(System.Drawing.Color.White);
            using var micPen = new Pen(System.Drawing.Color.White, 2.2f)
            {
                StartCap = LineCap.Round,
                EndCap = LineCap.Round
            };

            using var bodyPath = CreateRoundedRectanglePath(new RectangleF(11f, 7f, 10f, 11f), 5f);
            g.FillPath(micBrush, bodyPath);

            g.DrawArc(micPen, 8f, 11f, 16f, 11f, 0, 180);
            g.DrawLine(micPen, 16f, 22f, 16f, 24.5f);
            g.DrawLine(micPen, 12f, 24.5f, 20f, 24.5f);
        }

        private static void DrawDotTrayIcon(Graphics g, int size, System.Drawing.Color color)
        {
            const int dotSize = 20;
            int offset = (size - dotSize) / 2;
            using var brush = new SolidBrush(color);
            g.FillEllipse(brush, offset, offset, dotSize, dotSize);
        }

        private static GraphicsPath CreateRoundedRectanglePath(RectangleF rect, float radius)
        {
            var diameter = radius * 2;
            var path = new GraphicsPath();

            path.AddArc(rect.X, rect.Y, diameter, diameter, 180, 90);
            path.AddArc(rect.Right - diameter, rect.Y, diameter, diameter, 270, 90);
            path.AddArc(rect.Right - diameter, rect.Bottom - diameter, diameter, diameter, 0, 90);
            path.AddArc(rect.X, rect.Bottom - diameter, diameter, diameter, 90, 90);
            path.CloseFigure();

            return path;
        }

        private void PaneToggleButton_Click(object sender, RoutedEventArgs e)
        {
            NavView.IsPaneOpen = !NavView.IsPaneOpen;
        }

        private void NavView_PaneOpening(NavigationView sender, object args)
        {
            // Switch to expanded update notification
            if (_updateAvailable)
            {
                UpdateNotificationBorder.Visibility = Visibility.Visible;
                UpdateNotificationCompact.Visibility = Visibility.Collapsed;
            }
        }

        private void NavView_PaneClosing(NavigationView sender, NavigationViewPaneClosingEventArgs args)
        {
            // Switch to compact update notification (icon only)
            if (_updateAvailable)
            {
                UpdateNotificationBorder.Visibility = Visibility.Collapsed;
                UpdateNotificationCompact.Visibility = Visibility.Visible;
            }
        }

        private void NavView_SelectionChanged(NavigationView sender, NavigationViewSelectionChangedEventArgs args)
        {
            if (args.SelectedItem is NavigationViewItem item)
            {
                var tag = item.Tag?.ToString();
                if (tag == null || tag == _currentPage) return;

                // Determine slide direction
                var pageOrder = new[] { "General", "HoldToMute", "Sounds", "Overlay", "Appearance", "AutoMute", "About" };
                var currentIndex = Array.IndexOf(pageOrder, _currentPage);
                var newIndex = Array.IndexOf(pageOrder, tag);
                var effect = newIndex > currentIndex 
                    ? SlideNavigationTransitionEffect.FromRight 
                    : SlideNavigationTransitionEffect.FromLeft;
                NavigateToPage(tag, new SlideNavigationTransitionInfo { Effect = effect });
            }
        }

        private void MainWindow_Closed(object sender, WindowEventArgs args)
        {
            args.Handled = true;
            HideToTray();
        }

        public void HideToTray()
        {
            _appWindow?.Hide();
        }

        public void ShowWindow()
        {
            _appWindow?.Show();
            
            if (_appWindow != null)
            {
                var hwnd = WindowNative.GetWindowHandle(this);
                SetForegroundWindow(hwnd);
            }
        }

        [DllImport("user32.dll")]
        private static extern bool SetForegroundWindow(IntPtr hWnd);

        [DllImport("user32.dll", SetLastError = true)]
        private static extern bool DestroyIcon(IntPtr hIcon);

        [DllImport("user32.dll")]
        private static extern uint GetDpiForWindow(IntPtr hwnd);

        [DllImport("user32.dll")]
        private static extern int GetWindowLong(IntPtr hWnd, int nIndex);

        [DllImport("user32.dll")]
        private static extern int SetWindowLong(IntPtr hWnd, int nIndex, int dwNewLong);

        [DllImport("user32.dll")]
        private static extern IntPtr SetWindowLongPtr(IntPtr hWnd, int nIndex, IntPtr dwNewLong);

        [DllImport("user32.dll")]
        private static extern IntPtr CallWindowProc(IntPtr lpPrevWndFunc, IntPtr hWnd, uint Msg, IntPtr wParam, IntPtr lParam);

        private const int GWL_STYLE = -16;
        private const int GWL_WNDPROC = -4;
        private const int WS_MAXIMIZEBOX = 0x10000;
        private const uint WM_NCLBUTTONDBLCLK = 0x00A3;
        private const uint WM_SYSCOMMAND = 0x0112;
        private const uint WM_DPICHANGED = 0x02E0;
        private const int SC_MAXIMIZE = 0xF030;

        private IntPtr _oldWndProc = IntPtr.Zero;
        private delegate IntPtr WndProcDelegate(IntPtr hWnd, uint msg, IntPtr wParam, IntPtr lParam);
        private WndProcDelegate? _wndProcDelegate;
        private double _currentDpiScale = 1.0;

        private IntPtr WndProc(IntPtr hWnd, uint msg, IntPtr wParam, IntPtr lParam)
        {
            // Block maximize commands
            if (msg == WM_SYSCOMMAND && (wParam.ToInt32() & 0xFFF0) == SC_MAXIMIZE)
            {
                return IntPtr.Zero;
            }
            
            // Block double-click on title bar
            if (msg == WM_NCLBUTTONDBLCLK)
            {
                return IntPtr.Zero;
            }

            // Handle DPI changes (moving window between monitors with different scaling)
            if (msg == WM_DPICHANGED)
            {
                // Extract new DPI from wParam (high word = Y DPI, low word = X DPI)
                int newDpi = wParam.ToInt32() & 0xFFFF;
                double newDpiScale = newDpi / 96.0;
                
                // Only update if DPI actually changed
                if (Math.Abs(newDpiScale - _currentDpiScale) > 0.01)
                {
                    _currentDpiScale = newDpiScale;
                    
                    // Update minimum window size based on new DPI
                    DispatcherQueue.TryEnqueue(() =>
                    {
                        UpdateMinimumWindowSize();
                    });
                }
                
                // lParam contains suggested window rect - let Windows handle the resize
                // We'll just update our min size constraints
            }

            return CallWindowProc(_oldWndProc, hWnd, msg, wParam, lParam);
        }

        private void DisableMaximizeButton(IntPtr hwnd)
        {
            var style = GetWindowLong(hwnd, GWL_STYLE);
            SetWindowLong(hwnd, GWL_STYLE, style & ~WS_MAXIMIZEBOX);

            // Hook window procedure to block maximize messages
            _wndProcDelegate = new WndProcDelegate(WndProc);
            _oldWndProc = SetWindowLongPtr(hwnd, GWL_WNDPROC, Marshal.GetFunctionPointerForDelegate(_wndProcDelegate));
        }

        public void DisposeTrayIcon()
        {
            _trayIcon?.Dispose();
        }

        private void ApplyLocalizedStrings()
        {
            Title = AppResources.GetString("App.Title");
            if (_appWindow != null)
            {
                _appWindow.Title = AppResources.GetString("App.Title");
            }

            ToolTipService.SetToolTip(PaneToggleButton, AppResources.GetString("MainWindow.PaneToggleButton.ToolTipService.ToolTip"));
            ToolTipService.SetToolTip(UpdateNotificationCompact, AppResources.GetString("MainWindow.UpdateNotificationCompact.ToolTipService.ToolTip"));

            GeneralNavItem.Content = AppResources.GetString("MainWindow.GeneralNavItem.Content");
            AutoMuteNavItem.Content = AppResources.GetString("MainWindow.AutoMuteNavItem.Content");
            HoldToMuteNavItem.Content = AppResources.GetString("MainWindow.HoldToMuteNavItem.Content");
            SoundsNavItem.Content = AppResources.GetString("MainWindow.SoundsNavItem.Content");
            OverlayNavItem.Content = AppResources.GetString("MainWindow.OverlayNavItem.Content");
            AppearanceNavItem.Content = AppResources.GetString("MainWindow.AppearanceNavItem.Content");
            AboutNavItem.Content = AppResources.GetString("MainWindow.AboutNavItem.Content");
            UpdateAvailableText.Text = AppResources.GetString("MainWindow.UpdateAvailableText.Text");
            UpdateDetailsButton.Content = AppResources.GetString("MainWindow.UpdateDetailsButton.Content");

            if (_appInfoItem != null)
            {
                _appInfoItem.Text = AppResources.Format("Tray.Menu.AppInfo", UpdateService.CurrentVersion);
            }

            if (_showItem != null)
            {
                _showItem.Text = AppResources.GetString("Tray.Menu.ShowSettings");
            }

            if (_muteItem != null)
            {
                _muteItem.Text = AppResources.GetString("Tray.Menu.ToggleMute");
            }

            if (_refreshOverlayItem != null)
            {
                _refreshOverlayItem.Text = AppResources.GetString("Tray.Menu.RefreshOverlay");
            }

            if (_exitItem != null)
            {
                _exitItem.Text = AppResources.GetString("Tray.Menu.Exit");
            }
        }

        private void NavigateToPage(string tag, NavigationTransitionInfo? transitionInfo, bool forceReload = false)
        {
            Type? pageType = tag switch
            {
                "General" => typeof(GeneralPage),
                "AutoMute" => typeof(AutoMutePage),
                "HoldToMute" => typeof(HoldToMutePage),
                "Sounds" => typeof(SoundsPage),
                "Appearance" => typeof(AppearancePage),
                "Overlay" => typeof(OverlayPage),
                "About" => typeof(AboutPage),
                _ => null
            };

            if (pageType == null)
            {
                return;
            }

            if (forceReload || tag != _currentPage)
            {
                ContentFrame.Navigate(pageType, null, transitionInfo ?? new SuppressNavigationTransitionInfo());
            }

            _currentPage = tag;
        }
    }

    public class RelayCommand : System.Windows.Input.ICommand
    {
        private readonly Action _execute;

        public RelayCommand(Action execute)
        {
            _execute = execute;
        }

        #pragma warning disable CS0067 // Event is never used
        public event EventHandler? CanExecuteChanged;
#pragma warning restore CS0067

        public bool CanExecute(object? parameter) => true;

        public void Execute(object? parameter) => _execute();
    }
}
