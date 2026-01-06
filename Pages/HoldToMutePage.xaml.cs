using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using silence_.Services;

namespace silence_.Pages
{
    public sealed partial class HoldToMutePage : Page
    {
        private bool _isRecordingHoldHotkey;
        private int _recordedKeyCode;
        private ModifierKeys _recordedModifiers;

        public HoldToMutePage()
        {
            InitializeComponent();
            LoadSettings();
            
            // Subscribe to hotkey recording events
            if (App.Instance?.KeyboardHookService != null)
            {
                App.Instance.KeyboardHookService.KeyPressed += OnKeyPressed;
                App.Instance.KeyboardHookService.ModifiersChanged += OnModifiersChanged;
                App.Instance.KeyboardHookService.ModifierHoldProgress += OnModifierHoldProgress;
            }
        }

        private void LoadSettings()
        {
            var settings = App.Instance?.SettingsService.Settings;
            if (settings == null) return;

            // Load hold hotkey settings
            if (settings.HoldHotkeyCode > 0 || settings.HoldHotkeyModifiers != ModifierKeys.None)
            {
                HoldHotkeyTextBox.Text = VirtualKeys.GetHotkeyDisplayString(settings.HoldHotkeyCode, settings.HoldHotkeyModifiers);
            }
            else
            {
                HoldHotkeyTextBox.Text = "";
            }
            IgnoreHoldModifiersCheckBox.IsChecked = settings.IgnoreHoldModifiers;
            HoldPlaySoundsCheckBox.IsChecked = settings.HoldPlaySounds;
            HoldShowOverlayCheckBox.IsChecked = settings.HoldShowOverlay;
            
            // Load action mode
            var actionIndex = settings.HoldAction switch
            {
                "Toggle" => 0,
                "HoldToMute" => 1,
                "HoldToUnmute" => 2,
                _ => 0
            };
            HoldActionComboBox.SelectedIndex = actionIndex;
            UpdateActionDescription(settings.HoldAction);
        }

        private void UpdateActionDescription(string action)
        {
            ActionDescriptionText.Text = action switch
            {
                "Toggle" => "Toggle between muted/unmuted while holding",
                "HoldToMute" => "Mute while holding (unmute on release)",
                "HoldToUnmute" => "Unmute while holding (mute on release)",
                _ => "Toggle between muted/unmuted while holding"
            };
        }

        #region Event Handlers

        private void RecordHoldHotkeyButton_Click(object sender, RoutedEventArgs e)
        {
            if (_isRecordingHoldHotkey)
            {
                StopRecordingHoldHotkey();
            }
            else
            {
                StartRecordingHoldHotkey();
            }
        }

        private void ClearHoldHotkeyButton_Click(object sender, RoutedEventArgs e)
        {
            HoldHotkeyTextBox.Text = "";
            App.Instance?.SettingsService.UpdateHoldHotkey(0, ModifierKeys.None);
            App.Instance?.KeyboardHookService.UpdateHoldHotkey(0, ModifierKeys.None, IgnoreHoldModifiersCheckBox.IsChecked ?? true);
        }

        private void IgnoreHoldModifiersCheckBox_Changed(object sender, RoutedEventArgs e)
        {
            var ignore = IgnoreHoldModifiersCheckBox.IsChecked ?? true;
            App.Instance?.SettingsService.UpdateIgnoreHoldModifiers(ignore);
            
            var settings = App.Instance?.SettingsService.Settings;
            if (settings != null)
            {
                App.Instance?.KeyboardHookService.UpdateHoldHotkey(
                    settings.HoldHotkeyCode,
                    settings.HoldHotkeyModifiers,
                    ignore);
            }
        }

        private void HoldPlaySoundsCheckBox_Changed(object sender, RoutedEventArgs e)
        {
            var playSounds = HoldPlaySoundsCheckBox.IsChecked ?? true;
            App.Instance?.SettingsService.UpdateHoldPlaySounds(playSounds);
        }

        private void HoldShowOverlayCheckBox_Changed(object sender, RoutedEventArgs e)
        {
            var showOverlay = HoldShowOverlayCheckBox.IsChecked ?? true;
            App.Instance?.SettingsService.UpdateHoldShowOverlay(showOverlay);
        }

        private void HoldActionComboBox_SelectionChanged(object sender, SelectionChangedEventArgs e)
        {
            if (HoldActionComboBox.SelectedItem is ComboBoxItem item)
            {
                var action = item.Tag?.ToString() ?? "Toggle";
                App.Instance?.SettingsService.UpdateHoldAction(action);
                UpdateActionDescription(action);
            }
        }

        #endregion

        #region Recording

        private void StartRecordingHoldHotkey()
        {
            _isRecordingHoldHotkey = true;
            _recordedKeyCode = 0;
            _recordedModifiers = ModifierKeys.None;
            RecordHoldHotkeyButton.Content = "Cancel";
            HoldHotkeyTextBox.Text = "Press keys...";
            HoldHotkeyHintText.Visibility = Visibility.Visible;
            
            // Focus the TextBox to prevent Space from clicking the button
            HoldHotkeyTextBox.Focus(FocusState.Programmatic);
            
            if (App.Instance?.KeyboardHookService != null)
            {
                App.Instance.KeyboardHookService.ResetRecordingState();
                App.Instance.KeyboardHookService.IsRecording = true;
            }
        }

        private void StopRecordingHoldHotkey()
        {
            _isRecordingHoldHotkey = false;
            RecordHoldHotkeyButton.Content = "Record";
            HoldHotkeyHintText.Visibility = Visibility.Collapsed;
            HoldHotkeyProgressBar.Visibility = Visibility.Collapsed;
            
            if (App.Instance?.KeyboardHookService != null)
            {
                App.Instance.KeyboardHookService.IsRecording = false;
                App.Instance.KeyboardHookService.ResetRecordingState();
            }
        }

        private void OnModifiersChanged(ModifierKeys modifiers)
        {
            if (!_isRecordingHoldHotkey) return;

            DispatcherQueue.TryEnqueue(() =>
            {
                _recordedModifiers = modifiers;
                var display = VirtualKeys.GetHotkeyDisplayString(0, modifiers);
                var text = string.IsNullOrEmpty(display) ? "Press keys..." : display + " + ...";
                
                HoldHotkeyTextBox.Text = text;
            });
        }

        private void OnModifierHoldProgress(double progress)
        {
            if (!_isRecordingHoldHotkey) return;

            DispatcherQueue.TryEnqueue(() =>
            {
                HoldHotkeyProgressBar.Value = progress;
                HoldHotkeyProgressBar.Visibility = progress > 0 ? Visibility.Visible : Visibility.Collapsed;
            });
        }

        private void OnKeyPressed(int keyCode, ModifierKeys modifiers)
        {
            if (!_isRecordingHoldHotkey) return;

            DispatcherQueue.TryEnqueue(() =>
            {
                _recordedKeyCode = keyCode;
                _recordedModifiers = modifiers;

                var displayText = VirtualKeys.GetHotkeyDisplayString(keyCode, modifiers);
                
                HoldHotkeyTextBox.Text = displayText;
                StopRecordingHoldHotkey();

                App.Instance?.SettingsService.UpdateHoldHotkey(keyCode, modifiers);
                App.Instance?.KeyboardHookService.UpdateHoldHotkey(
                    keyCode,
                    modifiers,
                    IgnoreHoldModifiersCheckBox.IsChecked ?? true);
            });
        }

        #endregion
    }
}
