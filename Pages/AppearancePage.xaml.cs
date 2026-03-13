using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;

namespace silence_.Pages
{
    public sealed partial class AppearancePage : Page
    {
        private bool _isInitializing = true;

        public AppearancePage()
        {
            InitializeComponent();
            LoadSettings();
            _isInitializing = false;
        }

        private void LoadSettings()
        {
            var settings = App.Instance?.SettingsService.Settings;
            if (settings == null)
            {
                TrayIconStyleComboBox.SelectedIndex = 0;
                return;
            }

            foreach (ComboBoxItem item in TrayIconStyleComboBox.Items)
            {
                if (item.Tag?.ToString() == settings.TrayIconStyle)
                {
                    TrayIconStyleComboBox.SelectedItem = item;
                    break;
                }
            }

            if (TrayIconStyleComboBox.SelectedIndex < 0)
            {
                TrayIconStyleComboBox.SelectedIndex = 0;
            }
        }

        private void ThemeSelector_SelectionChanged(object sender, SelectionChangedEventArgs e)
        {
            // Theme switching logic can be added here
            if (ThemeSelector.SelectedItem is RadioButton selectedRadio)
            {
                var theme = selectedRadio.Tag?.ToString();
                // TODO: Apply theme
            }
        }

        private void TrayIconStyleComboBox_SelectionChanged(object sender, SelectionChangedEventArgs e)
        {
            if (_isInitializing) return;

            if (TrayIconStyleComboBox.SelectedItem is ComboBoxItem selectedItem)
            {
                var style = selectedItem.Tag?.ToString() ?? "Standard";
                App.Instance?.SettingsService.UpdateTrayIconStyle(style);
                App.Instance?.MainWindowInstance?.RefreshTrayIcon();
            }
        }
    }
}

