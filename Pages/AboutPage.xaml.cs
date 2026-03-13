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
        private bool _isInitializing;

        public AboutPage()
        {
            InitializeComponent();
            _isInitializing = true;
            ApplyLocalizedStrings();
            
            // Show current version
            VersionText.Text = $"v{UpdateService.CurrentVersion}";
            
            // Load settings
            var settings = App.Instance?.SettingsService.Settings;
            if (settings != null)
            {
                AutoCheckToggle.IsOn = settings.CheckForUpdatesOnStartup;
                SelectLanguage(settings.LanguageOverride);
            }
            else
            {
                SelectLanguage(LocalizationService.EnglishLanguage);
            }
            
            // Check if we already have an update check result (e.g., from startup check)
            var cachedResult = App.Instance?.LastUpdateCheckResult;
            if (cachedResult != null && cachedResult.Success && cachedResult.IsUpdateAvailable)
            {
                _lastCheckResult = cachedResult;
                ShowUpdateAvailable(cachedResult);
            }

            _isInitializing = false;
        }

        private void ApplyLocalizedStrings()
        {
            DescriptionTextBlock.Text = AppResources.GetString("AboutPage.DescriptionText.Text");
            LanguageTitleText.Text = AppResources.GetString("AboutPage.LanguageTitle.Text");

            UpdatesHeaderTextBlock.Text = AppResources.GetString("AboutPage.UpdatesHeaderText.Text");
            CheckingTextBlock.Text = AppResources.GetString("AboutPage.CheckingText.Text");
            UpToDateTextBlock.Text = AppResources.GetString("AboutPage.UpToDateText.Text");
            UpdateAvailableText.Text = AppResources.GetString("AboutPage.UpdateAvailableText.Text");
            NewVersionText.Text = AppResources.GetString("AboutPage.NewVersionText.Text");
            DownloadUpdateButton.Content = AppResources.GetString("AboutPage.DownloadUpdateButton.Content");
            ViewReleaseButton.Content = AppResources.GetString("AboutPage.ViewReleaseButton.Content");
            DownloadingText.Text = AppResources.GetString("AboutPage.DownloadingText.Text");
            ErrorText.Text = AppResources.GetString("AboutPage.ErrorText.Text");
            CheckUpdatesButton.Content = AppResources.GetString("AboutPage.CheckUpdatesButton.Content");
            AutoCheckToggle.Header = AppResources.GetString("AboutPage.AutoCheckToggle.Header");
            ViewOnGitHubButton.Content = AppResources.GetString("AboutPage.ViewOnGitHubButton.Content");
            MadeByTextBlock.Text = AppResources.GetString("AboutPage.MadeByText.Text");
            CreditsTitleText.Text = AppResources.GetString("AboutPage.CreditsTitle.Text");
            CreditWinUiTextBlock.Text = AppResources.GetString("AboutPage.CreditWinUiText.Text");
            CreditNotifyIconTextBlock.Text = AppResources.GetString("AboutPage.CreditNotifyIconText.Text");
        }
        
        private void ShowUpdateAvailable(UpdateCheckResult result)
        {
            HideAllPanels();
            UpdateAvailablePanel.Visibility = Visibility.Visible;
            NewVersionText.Text = AppResources.Format("Update.NewVersion", result.LatestVersion ?? string.Empty);
            
            // Hide the "Check for Updates" button when update is available
            CheckUpdatesButton.Visibility = Visibility.Collapsed;
            DownloadUpdateButton.Content = AppResources.GetString("AboutPage.DownloadUpdateButton.Content");
            
            // Disable download button if no installer found for current arch
            DownloadUpdateButton.IsEnabled = !string.IsNullOrEmpty(result.DownloadUrl);
            if (!DownloadUpdateButton.IsEnabled)
            {
                DownloadUpdateButton.Content = AppResources.GetString("Update.NoInstallerAvailable");
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
                    ErrorText.Text = _lastCheckResult.ErrorMessage ?? AppResources.GetString("Update.Error.Unknown");
                    CheckUpdatesButton.Visibility = Visibility.Visible;
                }
                else if (_lastCheckResult.IsUpdateAvailable)
                {
                    UpdateAvailablePanel.Visibility = Visibility.Visible;
                    NewVersionText.Text = AppResources.Format("Update.NewVersion", _lastCheckResult.LatestVersion ?? string.Empty);
                    
                    // Hide the "Check for Updates" button when update is available
                    CheckUpdatesButton.Visibility = Visibility.Collapsed;
                    DownloadUpdateButton.Content = AppResources.GetString("AboutPage.DownloadUpdateButton.Content");
                    
                    // Disable download button if no installer found for current arch
                    DownloadUpdateButton.IsEnabled = !string.IsNullOrEmpty(_lastCheckResult.DownloadUrl);
                    if (!DownloadUpdateButton.IsEnabled)
                    {
                        DownloadUpdateButton.Content = AppResources.GetString("Update.NoInstallerForPlatform");
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
                    DownloadingText.Text = AppResources.Format("Update.DownloadingProgress", percent);
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
                    DownloadingText.Text = AppResources.GetString("Update.StartingInstaller");
                    
                    // Launch installer and exit
                    UpdateService.LaunchInstallerAndExit(result.FilePath);
                }
                else
                {
                    HideAllPanels();
                    ErrorPanel.Visibility = Visibility.Visible;
                    ErrorText.Text = result.ErrorMessage ?? AppResources.GetString("Update.Error.DownloadFailed");
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

        private void LanguageComboBox_SelectionChanged(object sender, SelectionChangedEventArgs e)
        {
            if (_isInitializing) return;

            if (LanguageComboBox.SelectedItem is ComboBoxItem item)
            {
                App.Instance?.ApplyLanguageOverride(item.Tag?.ToString());
            }
        }

        private void HideAllPanels()
        {
            CheckingPanel.Visibility = Visibility.Collapsed;
            UpToDatePanel.Visibility = Visibility.Collapsed;
            UpdateAvailablePanel.Visibility = Visibility.Collapsed;
            DownloadingPanel.Visibility = Visibility.Collapsed;
            ErrorPanel.Visibility = Visibility.Collapsed;
        }

        private void SelectLanguage(string? languageOverride)
        {
            var resolvedLanguage = LocalizationService.ResolveAppLanguage(languageOverride);

            foreach (ComboBoxItem item in LanguageComboBox.Items)
            {
                if (string.Equals(item.Tag?.ToString(), resolvedLanguage, StringComparison.OrdinalIgnoreCase))
                {
                    LanguageComboBox.SelectedItem = item;
                    return;
                }
            }

            LanguageComboBox.SelectedIndex = 0;
        }
    }
}
