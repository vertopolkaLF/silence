using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Media.Animation;
using silence_.Services;
using System;

namespace silence_.Pages
{
    public sealed partial class GeneralPage : Page
    {
        private bool _isRecordingHotkey;
        private bool _isMuted;
        private int _recordedKeyCode;
        private ModifierKeys _recordedModifiers;
        private bool _isHovering;
        private bool _isFirstMuteUpdate = true;

        // Colors for mute button
        private static readonly Windows.UI.Color MutedColor = Windows.UI.Color.FromArgb(255, 205, 60, 70);
        private static readonly Windows.UI.Color MutedHoverColor = Windows.UI.Color.FromArgb(255, 160, 40, 50);
        private static readonly Windows.UI.Color UnmutedColor = Windows.UI.Color.FromArgb(255, 40, 167, 69);
        private static readonly Windows.UI.Color UnmutedHoverColor = Windows.UI.Color.FromArgb(255, 30, 130, 55);
        private static readonly TimeSpan AnimationDuration = TimeSpan.FromMilliseconds(200);

        public GeneralPage()
        {
            InitializeComponent();
            LoadSettings();
            
            // Subscribe to hotkey recording events
            if (App.Instance?.KeyboardHookService != null)
            {
                App.Instance.KeyboardHookService.KeyPressed += OnKeyPressed;
                App.Instance.KeyboardHookService.ModifiersChanged += OnModifiersChanged;
            }

            // Subscribe to mute state changes
            if (App.Instance != null)
            {
                App.Instance.MuteStateChanged += OnMuteStateChanged;
            }

            UpdateMuteState(App.Instance?.MicrophoneService.IsMuted() ?? false);
        }

        private void OnMuteStateChanged(bool isMuted)
        {
            DispatcherQueue.TryEnqueue(() => UpdateMuteState(isMuted));
        }

        private void LoadSettings()
        {
            var settings = App.Instance?.SettingsService.Settings;
            if (settings == null) return;

            RefreshMicrophones();

            HotkeyTextBox.Text = VirtualKeys.GetHotkeyDisplayString(settings.HotkeyCode, settings.HotkeyModifiers);
            IgnoreModifiersCheckBox.IsChecked = settings.IgnoreModifiers;

            AutoStartCheckBox.IsChecked = App.Instance?.SettingsService.IsAutoStartEnabled() ?? false;
            StartMinimizedCheckBox.IsChecked = settings.StartMinimized;
        }

        private void RefreshMicrophones()
        {
            var microphones = App.Instance?.MicrophoneService.GetMicrophones();
            if (microphones == null) return;

            MicrophoneComboBox.Items.Clear();
            
            var defaultItem = new ComboBoxItem { Content = "Default Microphone", Tag = (string?)null };
            MicrophoneComboBox.Items.Add(defaultItem);

            int selectedIndex = 0;
            var selectedId = App.Instance?.SettingsService.Settings.SelectedMicrophoneId;

            for (int i = 0; i < microphones.Count; i++)
            {
                var mic = microphones[i];
                var item = new ComboBoxItem 
                { 
                    Content = mic.IsDefault ? $"{mic.Name} (Default)" : mic.Name,
                    Tag = mic.Id 
                };
                MicrophoneComboBox.Items.Add(item);

                if (mic.Id == selectedId)
                {
                    selectedIndex = i + 1;
                }
            }

            MicrophoneComboBox.SelectedIndex = selectedIndex;
        }

        public void UpdateMuteState(bool isMuted)
        {
            var stateChanged = _isMuted != isMuted;
            _isMuted = isMuted;
            
            if (_isFirstMuteUpdate)
            {
                _isFirstMuteUpdate = false;
                MuteStatusText.Text = isMuted ? "muted" : "unmuted";
                MuteStatusTextAlt.Text = isMuted ? "unmuted" : "muted";
                MuteStatusText.Opacity = 1;
                MuteStatusTextAlt.Opacity = 0;
                MuteStatusTransform.TranslateY = 0;
                MuteStatusTransformAlt.TranslateY = isMuted ? -30 : 30;
            }
            else if (stateChanged)
            {
                AnimateMuteText(isMuted);
            }
            UpdateButtonColor();
        }

        private void AnimateMuteText(bool isMuted)
        {
            var storyboard = new Storyboard();
            var duration = TimeSpan.FromMilliseconds(280);
            var easing = new QuadraticEase { EasingMode = EasingMode.EaseInOut };

            double outDirection = isMuted ? -30 : 30;
            double inStartPos = isMuted ? 30 : -30;

            MuteStatusTextAlt.Text = isMuted ? "muted" : "unmuted";
            MuteStatusTransformAlt.TranslateY = inStartPos;
            MuteStatusTransformAlt.ScaleX = 0.8;
            MuteStatusTransformAlt.ScaleY = 0.8;
            
            var slideOut = new DoubleAnimation
            {
                To = outDirection,
                Duration = new Duration(duration),
                EasingFunction = easing
            };
            Storyboard.SetTarget(slideOut, MuteStatusTransform);
            Storyboard.SetTargetProperty(slideOut, "TranslateY");
            storyboard.Children.Add(slideOut);

            var fadeOut = new DoubleAnimation
            {
                To = 0,
                Duration = new Duration(duration),
                EasingFunction = easing
            };
            Storyboard.SetTarget(fadeOut, MuteStatusText);
            Storyboard.SetTargetProperty(fadeOut, "Opacity");
            storyboard.Children.Add(fadeOut);

            var scaleOutX = new DoubleAnimation
            {
                To = 0.8,
                Duration = new Duration(duration),
                EasingFunction = easing
            };
            Storyboard.SetTarget(scaleOutX, MuteStatusTransform);
            Storyboard.SetTargetProperty(scaleOutX, "ScaleX");
            storyboard.Children.Add(scaleOutX);

            var scaleOutY = new DoubleAnimation
            {
                To = 0.8,
                Duration = new Duration(duration),
                EasingFunction = easing
            };
            Storyboard.SetTarget(scaleOutY, MuteStatusTransform);
            Storyboard.SetTargetProperty(scaleOutY, "ScaleY");
            storyboard.Children.Add(scaleOutY);

            var slideIn = new DoubleAnimation
            {
                To = 0,
                Duration = new Duration(duration),
                EasingFunction = easing
            };
            Storyboard.SetTarget(slideIn, MuteStatusTransformAlt);
            Storyboard.SetTargetProperty(slideIn, "TranslateY");
            storyboard.Children.Add(slideIn);

            var fadeIn = new DoubleAnimation
            {
                To = 1,
                Duration = new Duration(duration),
                EasingFunction = easing
            };
            Storyboard.SetTarget(fadeIn, MuteStatusTextAlt);
            Storyboard.SetTargetProperty(fadeIn, "Opacity");
            storyboard.Children.Add(fadeIn);

            var scaleInX = new DoubleAnimation
            {
                To = 1,
                Duration = new Duration(duration),
                EasingFunction = easing
            };
            Storyboard.SetTarget(scaleInX, MuteStatusTransformAlt);
            Storyboard.SetTargetProperty(scaleInX, "ScaleX");
            storyboard.Children.Add(scaleInX);

            var scaleInY = new DoubleAnimation
            {
                To = 1,
                Duration = new Duration(duration),
                EasingFunction = easing
            };
            Storyboard.SetTarget(scaleInY, MuteStatusTransformAlt);
            Storyboard.SetTargetProperty(scaleInY, "ScaleY");
            storyboard.Children.Add(scaleInY);

            storyboard.Completed += (s, e) =>
            {
                MuteStatusText.Text = isMuted ? "muted" : "unmuted";
                MuteStatusText.Opacity = 1;
                MuteStatusTransform.TranslateY = 0;
                MuteStatusTransform.ScaleX = 1;
                MuteStatusTransform.ScaleY = 1;
                
                MuteStatusTextAlt.Opacity = 0;
                MuteStatusTransformAlt.TranslateY = isMuted ? -30 : 30;
            };

            storyboard.Begin();
        }

        private void UpdateButtonColor()
        {
            var color = _isMuted
                ? (_isHovering ? MutedHoverColor : MutedColor)
                : (_isHovering ? UnmutedHoverColor : UnmutedColor);
            
            AnimateButtonColor(color);
        }

        private void AnimateButtonColor(Windows.UI.Color targetColor)
        {
            var currentBrush = MuteButton.Background as SolidColorBrush;
            if (currentBrush == null)
            {
                MuteButton.Background = new SolidColorBrush(targetColor);
                return;
            }

            var storyboard = new Storyboard();
            var animation = new ColorAnimation
            {
                To = targetColor,
                Duration = new Duration(AnimationDuration),
                EasingFunction = new QuadraticEase { EasingMode = EasingMode.EaseOut }
            };

            Storyboard.SetTarget(animation, MuteButton);
            Storyboard.SetTargetProperty(animation, "(Border.Background).(SolidColorBrush.Color)");
            storyboard.Children.Add(animation);
            storyboard.Begin();
        }

        #region Event Handlers

        private void MuteButton_Click(object sender, Microsoft.UI.Xaml.Input.TappedRoutedEventArgs e)
        {
            App.Instance?.ToggleMute();
        }

        private void MuteButton_PointerEntered(object sender, Microsoft.UI.Xaml.Input.PointerRoutedEventArgs e)
        {
            _isHovering = true;
            UpdateButtonColor();
        }

        private void MuteButton_PointerExited(object sender, Microsoft.UI.Xaml.Input.PointerRoutedEventArgs e)
        {
            _isHovering = false;
            UpdateButtonColor();
        }

        private void MuteButton_PointerPressed(object sender, Microsoft.UI.Xaml.Input.PointerRoutedEventArgs e)
        {
            _isHovering = true;
            UpdateButtonColor();
        }

        private void MuteButton_PointerReleased(object sender, Microsoft.UI.Xaml.Input.PointerRoutedEventArgs e)
        {
            UpdateButtonColor();
        }

        private void MicrophoneComboBox_SelectionChanged(object sender, SelectionChangedEventArgs e)
        {
            if (MicrophoneComboBox.SelectedItem is ComboBoxItem item)
            {
                var deviceId = item.Tag as string;
                App.Instance?.MicrophoneService.SelectMicrophone(deviceId);
                App.Instance?.SettingsService.UpdateSelectedMicrophone(deviceId);
            }
        }

        private void RecordHotkeyButton_Click(object sender, RoutedEventArgs e)
        {
            if (_isRecordingHotkey)
            {
                StopRecordingHotkey();
            }
            else
            {
                StartRecordingHotkey();
            }
        }

        private void StartRecordingHotkey()
        {
            _isRecordingHotkey = true;
            _recordedKeyCode = 0;
            _recordedModifiers = ModifierKeys.None;
            RecordHotkeyButton.Content = "Cancel";
            HotkeyTextBox.Text = "Press keys...";
            
            if (App.Instance?.KeyboardHookService != null)
            {
                App.Instance.KeyboardHookService.ResetRecordingState();
                App.Instance.KeyboardHookService.IsRecording = true;
            }
        }

        private void StopRecordingHotkey()
        {
            _isRecordingHotkey = false;
            RecordHotkeyButton.Content = "Record";
            
            if (App.Instance?.KeyboardHookService != null)
            {
                App.Instance.KeyboardHookService.IsRecording = false;
                App.Instance.KeyboardHookService.ResetRecordingState();
            }
        }

        private void OnModifiersChanged(ModifierKeys modifiers)
        {
            if (!_isRecordingHotkey) return;

            DispatcherQueue.TryEnqueue(() =>
            {
                _recordedModifiers = modifiers;
                var display = VirtualKeys.GetHotkeyDisplayString(0, modifiers);
                HotkeyTextBox.Text = string.IsNullOrEmpty(display) ? "Press keys..." : display + " + ...";
            });
        }

        private void OnKeyPressed(int keyCode, ModifierKeys modifiers)
        {
            if (!_isRecordingHotkey) return;

            DispatcherQueue.TryEnqueue(() =>
            {
                _recordedKeyCode = keyCode;
                _recordedModifiers = modifiers;

                HotkeyTextBox.Text = VirtualKeys.GetHotkeyDisplayString(keyCode, modifiers);
                StopRecordingHotkey();

                App.Instance?.SettingsService.UpdateHotkey(keyCode, modifiers);
                App.Instance?.KeyboardHookService.UpdateHotkey(
                    keyCode,
                    modifiers,
                    IgnoreModifiersCheckBox.IsChecked ?? true);
            });
        }

        private void IgnoreModifiersCheckBox_Changed(object sender, RoutedEventArgs e)
        {
            var ignore = IgnoreModifiersCheckBox.IsChecked ?? true;
            App.Instance?.SettingsService.UpdateIgnoreModifiers(ignore);
            
            var settings = App.Instance?.SettingsService.Settings;
            if (settings != null)
            {
                App.Instance?.KeyboardHookService.UpdateHotkey(
                    settings.HotkeyCode,
                    settings.HotkeyModifiers,
                    ignore);
            }
        }

        private void AutoStartCheckBox_Changed(object sender, RoutedEventArgs e)
        {
            var enabled = AutoStartCheckBox.IsChecked ?? false;
            App.Instance?.SettingsService.SetAutoStart(enabled);
        }

        private void StartMinimizedCheckBox_Changed(object sender, RoutedEventArgs e)
        {
            var minimized = StartMinimizedCheckBox.IsChecked ?? true;
            App.Instance?.SettingsService.UpdateStartMinimized(minimized);
        }

        private void HideButton_Click(object sender, RoutedEventArgs e)
        {
            App.Instance?.HideMainWindow();
        }

        private void ExitButton_Click(object sender, RoutedEventArgs e)
        {
            App.Instance?.ExitApplication();
        }

        #endregion
    }
}

