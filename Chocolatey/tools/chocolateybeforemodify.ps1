$ErrorActionPreference = 'Stop'

# Stop silence! process if it's running before upgrade/uninstall
$processName = "silence!"
$process = Get-Process -Name $processName -ErrorAction SilentlyContinue

if ($process) {
  Write-Host "Stopping $processName process..."
  $process | Stop-Process -Force
  Start-Sleep -Seconds 2
}
