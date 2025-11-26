using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;

namespace silence_.Pages
{
    public sealed partial class AppearancePage : Page
    {
        public AppearancePage()
        {
            InitializeComponent();
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
    }
}

