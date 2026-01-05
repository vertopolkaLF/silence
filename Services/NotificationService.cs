using Microsoft.Windows.AppNotifications;
using System;
using System.Diagnostics;

namespace silence_.Services;

/// <summary>
/// Service for sending Windows app notifications (toast notifications)
/// </summary>
public class NotificationService : IDisposable
{
    private AppNotificationManager? _notificationManager;
    private bool _disposed;
    private bool _isRegistered;

    public NotificationService()
    {
        try
        {
            Debug.WriteLine("NotificationService: Initializing...");
            _notificationManager = AppNotificationManager.Default;
            Debug.WriteLine("NotificationService: Got AppNotificationManager.Default");
            
            _notificationManager.NotificationInvoked += OnNotificationInvoked;
            Debug.WriteLine("NotificationService: Subscribed to NotificationInvoked");
            
            // Register for notifications
            _notificationManager.Register();
            _isRegistered = true;
            
            Debug.WriteLine("NotificationService: Successfully registered");
        }
        catch (Exception ex)
        {
            Debug.WriteLine($"NotificationService: Failed to register - {ex.GetType().Name}: {ex.Message}");
            Debug.WriteLine($"NotificationService: Stack trace: {ex.StackTrace}");
            _isRegistered = false;
        }
    }

    /// <summary>
    /// Send an update available notification with action buttons
    /// </summary>
    public void SendUpdateNotification(UpdateCheckResult updateResult)
    {
        Debug.WriteLine($"NotificationService.SendUpdateNotification called - IsRegistered: {_isRegistered}");
        
        if (!_isRegistered || _notificationManager == null)
        {
            Debug.WriteLine("NotificationService: Not registered or manager is null, cannot send notification");
            return;
        }

        if (!updateResult.IsUpdateAvailable || string.IsNullOrEmpty(updateResult.LatestVersion))
        {
            Debug.WriteLine("NotificationService: No update available or version is empty");
            return;
        }

        try
        {
            Debug.WriteLine($"NotificationService: Building notification for v{updateResult.LatestVersion}");
            
            // Escape XML special characters
            var releaseUrl = System.Security.SecurityElement.Escape(updateResult.ReleaseUrl ?? "");
            var downloadUrl = System.Security.SecurityElement.Escape(updateResult.DownloadUrl ?? "");
            var fileName = System.Security.SecurityElement.Escape(updateResult.InstallerFileName ?? "");
            var version = System.Security.SecurityElement.Escape(updateResult.LatestVersion);
            
            // Use XML directly for better compatibility
            var toastXml = $@"<toast launch=""action=openAbout"">
    <visual>
        <binding template=""ToastGeneric"">
            <text>Update Available: v{version}</text>
            <text>A new version of silence! is ready to install</text>
        </binding>
    </visual>
    <actions>
        <action content=""Install Update"" arguments=""action=installUpdate;downloadUrl={downloadUrl};fileName={fileName}"" />
    </actions>
</toast>";

            Debug.WriteLine($"NotificationService: Toast XML created");
            
            var notification = new AppNotification(toastXml);
            
            Debug.WriteLine("NotificationService: Created AppNotification, calling Show()...");
            _notificationManager.Show(notification);
            Debug.WriteLine("NotificationService: Show() completed successfully");
        }
        catch (Exception ex)
        {
            Debug.WriteLine($"NotificationService: Failed to send notification - {ex.GetType().Name}: {ex.Message}");
            Debug.WriteLine($"NotificationService: Stack trace: {ex.StackTrace}");
        }
    }

    private void OnNotificationInvoked(AppNotificationManager sender, AppNotificationActivatedEventArgs args)
    {
        try
        {
            Debug.WriteLine("NotificationService: Notification invoked");
            Debug.WriteLine($"NotificationService: Arguments: {args.Argument}");
            
            // Parse arguments manually from the string
            var argString = args.Argument;
            var argPairs = argString.Split(';');
            
            string? action = null;
            string? url = null;
            string? downloadUrl = null;
            string? fileName = null;
            
            foreach (var pair in argPairs)
            {
                var parts = pair.Split('=', 2);
                if (parts.Length == 2)
                {
                    var key = parts[0];
                    var value = parts[1];
                    
                    switch (key)
                    {
                        case "action": action = value; break;
                        case "url": url = value; break;
                        case "downloadUrl": downloadUrl = value; break;
                        case "fileName": fileName = value; break;
                    }
                }
            }

            Debug.WriteLine($"NotificationService: Parsed action = {action}");

            switch (action)
            {
                case "openAbout":
                    Debug.WriteLine("NotificationService: Opening About page");
                    App.Instance?.MainWindowInstance?.DispatcherQueue.TryEnqueue(() =>
                    {
                        App.Instance?.MainWindowInstance?.ShowWindow();
                        App.Instance?.MainWindowInstance?.NavigateToAbout();
                    });
                    break;

                case "whatsNew":
                    Debug.WriteLine($"NotificationService: Opening release page: {url}");
                    if (!string.IsNullOrEmpty(url))
                    {
                        UpdateService.OpenReleasesPage(url);
                    }
                    break;

                case "installUpdate":
                    Debug.WriteLine($"NotificationService: Installing update from {downloadUrl}");
                    if (!string.IsNullOrEmpty(downloadUrl) && !string.IsNullOrEmpty(fileName))
                    {
                        _ = DownloadAndInstallUpdateAsync(downloadUrl, fileName);
                    }
                    break;
            }
        }
        catch (Exception ex)
        {
            Debug.WriteLine($"NotificationService: Notification invoked error - {ex.GetType().Name}: {ex.Message}");
            Debug.WriteLine($"NotificationService: Stack trace: {ex.StackTrace}");
        }
    }

    private async System.Threading.Tasks.Task DownloadAndInstallUpdateAsync(string downloadUrl, string fileName)
    {
        try
        {
            Debug.WriteLine($"NotificationService: Starting download - {downloadUrl}");
            var updateService = new UpdateService();
            var result = await updateService.DownloadUpdateAsync(downloadUrl, fileName);

            if (result.Success && !string.IsNullOrEmpty(result.FilePath))
            {
                Debug.WriteLine($"NotificationService: Download successful, launching installer");
                UpdateService.LaunchInstallerAndExit(result.FilePath);
            }
            else
            {
                Debug.WriteLine($"NotificationService: Download failed - {result.ErrorMessage}");
                // Show error notification
                if (_isRegistered && _notificationManager != null)
                {
                    var errorXml = @"
<toast>
    <visual>
        <binding template=""ToastGeneric"">
            <text>Update Installation Failed</text>
            <text>" + (result.ErrorMessage ?? "Failed to download the update") + @"</text>
        </binding>
    </visual>
</toast>";
                    _notificationManager.Show(new AppNotification(errorXml));
                }
            }
        }
        catch (Exception ex)
        {
            Debug.WriteLine($"NotificationService: Download and install error - {ex.GetType().Name}: {ex.Message}");
        }
    }

    public void Dispose()
    {
        if (!_disposed)
        {
            if (_isRegistered && _notificationManager != null)
            {
                try
                {
                    _notificationManager.NotificationInvoked -= OnNotificationInvoked;
                    _notificationManager.Unregister();
                    Debug.WriteLine("NotificationService: Unregistered successfully");
                }
                catch (Exception ex)
                {
                    Debug.WriteLine($"NotificationService: Failed to unregister - {ex.Message}");
                }
            }
            _disposed = true;
        }
    }
}
