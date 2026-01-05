using System;
using System.Diagnostics;
using System.IO;
using System.Net.Http;
using System.Net.Http.Headers;
using System.Reflection;
using System.Runtime.InteropServices;
using System.Text.Json;
using System.Threading.Tasks;

namespace silence_.Services;

/// <summary>
/// Service for checking and downloading updates from GitHub releases
/// </summary>
public class UpdateService : IDisposable
{
    private const string GitHubApiUrl = "https://api.github.com/repos/vertopolkaLF/silence/releases/latest";
    private const string GitHubReleasesUrl = "https://github.com/vertopolkaLF/silence/releases/latest";
    
    private readonly HttpClient _httpClient;
    private bool _disposed;

    public static string CurrentVersion => GetCurrentVersion();

    public UpdateService()
    {
        _httpClient = new HttpClient();
        _httpClient.DefaultRequestHeaders.UserAgent.Add(
            new ProductInfoHeaderValue("silence", CurrentVersion));
        _httpClient.DefaultRequestHeaders.Accept.Add(
            new MediaTypeWithQualityHeaderValue("application/vnd.github.v3+json"));
    }

    private static string GetCurrentVersion()
    {
        var assembly = Assembly.GetExecutingAssembly();
        var version = assembly.GetName().Version;
        if (version == null) return "1.0";
        
        // Show Build part only if it's not 0 (e.g., "1.3" or "1.3.1")
        return version.Build > 0 
            ? $"{version.Major}.{version.Minor}.{version.Build}" 
            : $"{version.Major}.{version.Minor}";
    }

    /// <summary>
    /// Check if a newer version is available on GitHub
    /// </summary>
    public async Task<UpdateCheckResult> CheckForUpdatesAsync()
    {
        try
        {
            var response = await _httpClient.GetAsync(GitHubApiUrl);
            
            if (!response.IsSuccessStatusCode)
            {
                return new UpdateCheckResult
                {
                    Success = false,
                    ErrorMessage = $"GitHub API returned {response.StatusCode}"
                };
            }

            var json = await response.Content.ReadAsStringAsync();
            var release = JsonSerializer.Deserialize<GitHubRelease>(json);

            if (release == null)
            {
                return new UpdateCheckResult
                {
                    Success = false,
                    ErrorMessage = "Failed to parse GitHub response"
                };
            }

            var latestVersion = release.TagName?.TrimStart('v', 'V') ?? "0.0.0";
            var isUpdateAvailable = IsNewerVersion(latestVersion, CurrentVersion);

            // Find the appropriate installer asset for current architecture
            string? downloadUrl = null;
            string? assetName = null;
            
            if (release.Assets != null)
            {
                var arch = GetCurrentArchitecture();
                foreach (var asset in release.Assets)
                {
                    if (asset.Name != null && 
                        asset.Name.Contains(arch, StringComparison.OrdinalIgnoreCase) &&
                        asset.Name.EndsWith("-Setup.exe", StringComparison.OrdinalIgnoreCase))
                    {
                        downloadUrl = asset.BrowserDownloadUrl;
                        assetName = asset.Name;
                        break;
                    }
                }
            }

            return new UpdateCheckResult
            {
                Success = true,
                IsUpdateAvailable = isUpdateAvailable,
                CurrentVersion = CurrentVersion,
                LatestVersion = latestVersion,
                ReleaseNotes = release.Body,
                ReleaseName = release.Name,
                ReleaseUrl = release.HtmlUrl ?? GitHubReleasesUrl,
                DownloadUrl = downloadUrl,
                InstallerFileName = assetName
            };
        }
        catch (HttpRequestException ex)
        {
            return new UpdateCheckResult
            {
                Success = false,
                ErrorMessage = $"Network error: {ex.Message}"
            };
        }
        catch (TaskCanceledException)
        {
            return new UpdateCheckResult
            {
                Success = false,
                ErrorMessage = "Request timed out"
            };
        }
        catch (Exception ex)
        {
            return new UpdateCheckResult
            {
                Success = false,
                ErrorMessage = $"Unexpected error: {ex.Message}"
            };
        }
    }

    /// <summary>
    /// Download the installer to temp folder and return the path
    /// </summary>
    public async Task<DownloadResult> DownloadUpdateAsync(string downloadUrl, string fileName, IProgress<double>? progress = null)
    {
        try
        {
            var tempPath = Path.Combine(Path.GetTempPath(), fileName);

            using var response = await _httpClient.GetAsync(downloadUrl, HttpCompletionOption.ResponseHeadersRead);
            response.EnsureSuccessStatusCode();

            var totalBytes = response.Content.Headers.ContentLength ?? -1;
            var downloadedBytes = 0L;

            await using var contentStream = await response.Content.ReadAsStreamAsync();
            await using var fileStream = new FileStream(tempPath, FileMode.Create, FileAccess.Write, FileShare.None, 8192, true);

            var buffer = new byte[8192];
            int bytesRead;

            while ((bytesRead = await contentStream.ReadAsync(buffer)) > 0)
            {
                await fileStream.WriteAsync(buffer.AsMemory(0, bytesRead));
                downloadedBytes += bytesRead;

                if (totalBytes > 0)
                {
                    progress?.Report((double)downloadedBytes / totalBytes * 100);
                }
            }

            return new DownloadResult
            {
                Success = true,
                FilePath = tempPath
            };
        }
        catch (Exception ex)
        {
            return new DownloadResult
            {
                Success = false,
                ErrorMessage = ex.Message
            };
        }
    }

    /// <summary>
    /// Launch the installer and exit the application
    /// </summary>
    public static void LaunchInstallerAndExit(string installerPath)
    {
        try
        {
            var startInfo = new ProcessStartInfo
            {
                FileName = installerPath,
                Arguments = "/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /CLOSEAPPLICATIONS /RESTARTAPPLICATIONS",
                UseShellExecute = true
            };
            
            Process.Start(startInfo);
            
            // Give the installer a moment to start
            System.Threading.Thread.Sleep(500);
            
            // Exit the app so installer can replace files
            App.Instance?.ExitApplication();
        }
        catch (Exception ex)
        {
            Debug.WriteLine($"Failed to launch installer: {ex.Message}");
            throw;
        }
    }

    /// <summary>
    /// Open the releases page in browser
    /// </summary>
    public static void OpenReleasesPage(string? url = null)
    {
        try
        {
            var startInfo = new ProcessStartInfo
            {
                FileName = url ?? GitHubReleasesUrl,
                UseShellExecute = true
            };
            Process.Start(startInfo);
        }
        catch (Exception ex)
        {
            Debug.WriteLine($"Failed to open browser: {ex.Message}");
        }
    }

    private static string GetCurrentArchitecture()
    {
        return RuntimeInformation.ProcessArchitecture switch
        {
            Architecture.X64 => "x64",
            Architecture.X86 => "x86",
            Architecture.Arm64 => "arm64",
            _ => "x64"
        };
    }

    private static bool IsNewerVersion(string latest, string current)
    {
        try
        {
            var latestParts = latest.Split('.');
            var currentParts = current.Split('.');

            for (int i = 0; i < Math.Max(latestParts.Length, currentParts.Length); i++)
            {
                var latestPart = i < latestParts.Length ? int.Parse(latestParts[i]) : 0;
                var currentPart = i < currentParts.Length ? int.Parse(currentParts[i]) : 0;

                if (latestPart > currentPart) return true;
                if (latestPart < currentPart) return false;
            }

            return false;
        }
        catch
        {
            return false;
        }
    }

    public void Dispose()
    {
        if (!_disposed)
        {
            _httpClient.Dispose();
            _disposed = true;
        }
    }
}

public class UpdateCheckResult
{
    public bool Success { get; set; }
    public bool IsUpdateAvailable { get; set; }
    public string? CurrentVersion { get; set; }
    public string? LatestVersion { get; set; }
    public string? ReleaseNotes { get; set; }
    public string? ReleaseName { get; set; }
    public string? ReleaseUrl { get; set; }
    public string? DownloadUrl { get; set; }
    public string? InstallerFileName { get; set; }
    public string? ErrorMessage { get; set; }
}

public class DownloadResult
{
    public bool Success { get; set; }
    public string? FilePath { get; set; }
    public string? ErrorMessage { get; set; }
}

// GitHub API response models
public class GitHubRelease
{
    [System.Text.Json.Serialization.JsonPropertyName("tag_name")]
    public string? TagName { get; set; }

    [System.Text.Json.Serialization.JsonPropertyName("name")]
    public string? Name { get; set; }

    [System.Text.Json.Serialization.JsonPropertyName("body")]
    public string? Body { get; set; }

    [System.Text.Json.Serialization.JsonPropertyName("html_url")]
    public string? HtmlUrl { get; set; }

    [System.Text.Json.Serialization.JsonPropertyName("assets")]
    public GitHubAsset[]? Assets { get; set; }
}

public class GitHubAsset
{
    [System.Text.Json.Serialization.JsonPropertyName("name")]
    public string? Name { get; set; }

    [System.Text.Json.Serialization.JsonPropertyName("browser_download_url")]
    public string? BrowserDownloadUrl { get; set; }

    [System.Text.Json.Serialization.JsonPropertyName("size")]
    public long Size { get; set; }
}

