using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using silence_.Services;
using System;

namespace silence_.Pages
{
    public sealed partial class AboutPage : Page
    {
        private UpdateService? _updateService;
        private UpdateCheckResult? _lastCheckResult;

        public AboutPage()
        {
            InitializeComponent();
            
            // Show current version
            VersionText.Text = $"v{UpdateService.CurrentVersion}";
            
            // Load settings
            var settings = App.Instance?.SettingsService.Settings;
            if (settings != null)
            {
                AutoCheckToggle.IsOn = settings.CheckForUpdatesOnStartup;
            }
            
            // Check if we already have an update check result (e.g., from startup check)
            var cachedResult = App.Instance?.LastUpdateCheckResult;
            if (cachedResult != null && cachedResult.Success && cachedResult.IsUpdateAvailable)
            {
                _lastCheckResult = cachedResult;
                ShowUpdateAvailable(cachedResult);
            }
        }
        
        private void ShowUpdateAvailable(UpdateCheckResult result)
        {
            HideAllPanels();
            UpdateAvailablePanel.Visibility = Visibility.Visible;
            NewVersionText.Text = $"New version: v{result.LatestVersion}";
            
            // Hide the "Check for Updates" button when update is available
            CheckUpdatesButton.Visibility = Visibility.Collapsed;
            
            // Disable download button if no installer found for current arch
            DownloadUpdateButton.IsEnabled = !string.IsNullOrEmpty(result.DownloadUrl);
            if (!DownloadUpdateButton.IsEnabled)
            {
                DownloadUpdateButton.Content = "No installer available";
            }
        }

        private async void CheckUpdatesButton_Click(object sender, RoutedEventArgs e)
        {
            await CheckForUpdates();
        }

        private async System.Threading.Tasks.Task CheckForUpdates()
        {
            _updateService ??= new UpdateService();

            // Hide all status panels
            HideAllPanels();
            CheckingPanel.Visibility = Visibility.Visible;
            CheckUpdatesButton.IsEnabled = false;

            try
            {
                _lastCheckResult = await _updateService.CheckForUpdatesAsync();

                HideAllPanels();

                if (!_lastCheckResult.Success)
                {
                    ErrorPanel.Visibility = Visibility.Visible;
                    ErrorText.Text = _lastCheckResult.ErrorMessage ?? "Unknown error";
                    CheckUpdatesButton.Visibility = Visibility.Visible;
                }
                else if (_lastCheckResult.IsUpdateAvailable)
                {
                    UpdateAvailablePanel.Visibility = Visibility.Visible;
                    NewVersionText.Text = $"New version: v{_lastCheckResult.LatestVersion}";
                    
                    // Hide the "Check for Updates" button when update is available
                    CheckUpdatesButton.Visibility = Visibility.Collapsed;
                    
                    // Disable download button if no installer found for current arch
                    DownloadUpdateButton.IsEnabled = !string.IsNullOrEmpty(_lastCheckResult.DownloadUrl);
                    if (!DownloadUpdateButton.IsEnabled)
                    {
                        DownloadUpdateButton.Content = "No installer for your platform";
                    }
                }
                else
                {
                    UpToDatePanel.Visibility = Visibility.Visible;
                    CheckUpdatesButton.Visibility = Visibility.Visible;
                }

                // Update last check time
                App.Instance?.SettingsService.UpdateLastUpdateCheck();
            }
            catch (Exception ex)
            {
                HideAllPanels();
                ErrorPanel.Visibility = Visibility.Visible;
                ErrorText.Text = ex.Message;
            }
            finally
            {
                CheckUpdatesButton.IsEnabled = true;
            }
        }

        private async void DownloadUpdateButton_Click(object sender, RoutedEventArgs e)
        {
            if (_lastCheckResult == null || 
                string.IsNullOrEmpty(_lastCheckResult.DownloadUrl) ||
                string.IsNullOrEmpty(_lastCheckResult.InstallerFileName))
            {
                return;
            }

            _updateService ??= new UpdateService();

            HideAllPanels();
            DownloadingPanel.Visibility = Visibility.Visible;
            CheckUpdatesButton.IsEnabled = false;

            var progress = new Progress<double>(percent =>
            {
                DispatcherQueue.TryEnqueue(() =>
                {
                    DownloadProgress.Value = percent;
                    DownloadingText.Text = $"Downloading... {percent:F0}%";
                });
            });

            try
            {
                var result = await _updateService.DownloadUpdateAsync(
                    _lastCheckResult.DownloadUrl,
                    _lastCheckResult.InstallerFileName,
                    progress);

                if (result.Success && !string.IsNullOrEmpty(result.FilePath))
                {
                    DownloadingText.Text = "Starting installer...";
                    
                    // Launch installer and exit
                    UpdateService.LaunchInstallerAndExit(result.FilePath);
                }
                else
                {
                    HideAllPanels();
                    ErrorPanel.Visibility = Visibility.Visible;
                    ErrorText.Text = result.ErrorMessage ?? "Download failed";
                }
            }
            catch (Exception ex)
            {
                HideAllPanels();
                ErrorPanel.Visibility = Visibility.Visible;
                ErrorText.Text = ex.Message;
            }
            finally
            {
                CheckUpdatesButton.IsEnabled = true;
            }
        }

        private void ViewReleaseButton_Click(object sender, RoutedEventArgs e)
        {
            UpdateService.OpenReleasesPage(_lastCheckResult?.ReleaseUrl);
        }

        private void AutoCheckToggle_Toggled(object sender, RoutedEventArgs e)
        {
            App.Instance?.SettingsService.UpdateCheckForUpdatesOnStartup(AutoCheckToggle.IsOn);
        }

        private void HideAllPanels()
        {
            CheckingPanel.Visibility = Visibility.Collapsed;
            UpToDatePanel.Visibility = Visibility.Collapsed;
            UpdateAvailablePanel.Visibility = Visibility.Collapsed;
            DownloadingPanel.Visibility = Visibility.Collapsed;
            ErrorPanel.Visibility = Visibility.Collapsed;
        }
    }
}
