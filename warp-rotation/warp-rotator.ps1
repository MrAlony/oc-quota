param(
  [switch]$SkipMonitor,
  [switch]$ForceRotate,
  [switch]$Logs,
  [switch]$InternalHidden,
  [int]$PollInterval = 15,
  [string]$LogFile = "",
  [string]$NineRouterUrl = "http://localhost:20128"
)

# Auto-relaunch hidden when running without -Logs
if (-not $Logs -and -not $InternalHidden) {
  $argList = "-ExecutionPolicy Bypass -File `"$PSCommandPath`""
  if ($SkipMonitor) { $argList += " -SkipMonitor" }
  if ($ForceRotate) { $argList += " -ForceRotate" }
  $argList += " -InternalHidden"
  Start-Process powershell -WindowStyle Hidden -ArgumentList $argList
  exit
}

$WarpCli     = "C:\Program Files\Cloudflare\Cloudflare WARP\warp-cli.exe"
$ProxyPoolId = "0d130c94-d89a-4e86-a08b-0e3337dede81"
$DataDir     = "$env:LOCALAPPDATA\Cloudflare\Warp"
$StateFile   = "$PSScriptRoot\.last-error.txt"
$LogPath     = if ($LogFile) { $LogFile } else { "$PSScriptRoot\warp-rotator.log" }

function Log { param([string]$Msg) $t = Get-Date -Format "yyyy-MM-dd HH:mm:ss"; "$t $Msg" | Out-File -FilePath $LogPath -Append; if ($Logs) { Write-Host "$t $Msg" } }

function Get-WarpStatus {
  try {
    $r = & $WarpCli status 2>&1
    if ($r -like "*Connected*") { return "connected" }
    if ($r -like "*Disconnected*") { return "disconnected" }
    return "unknown"
  } catch { return "error" }
}

function Get-WarpIp {
  try {
    return (curl.exe --socks5-hostname 127.0.0.1:40000 -s https://ifconfig.me/ip 2>$null)
  } catch { return $null }
}

function Get-LastErrorTimestamp {
  if (Test-Path $StateFile) { return (Get-Content $StateFile -Raw).Trim() }
  return ""
}

function Save-LastErrorTimestamp { param([string]$ts) Set-Content -Path $StateFile -Value $ts -NoNewline }

function Invoke-WarpRotate {
  Log "=== Starting Warp IP rotation ==="
  $oldIp = Get-WarpIp
  Log "Current Warp IP: $oldIp"

  Log "Step 1: Disconnect Warp"
  & $WarpCli disconnect 2>&1 | Out-Null
  Start-Sleep -Seconds 2

  Log "Step 2: Clear Warp registration data (force new IP)"
  $partialsDir = "$DataDir\warp-diag-partials"
  if (Test-Path $partialsDir) {
    Remove-Item -Path "$partialsDir\*" -Force -ErrorAction SilentlyContinue
    Log "Cleared warp-diag-partials"
  }
  Start-Sleep -Seconds 1

  Log "Step 3: Re-register Warp"
  & $WarpCli register 2>&1 | Out-Null
  Start-Sleep -Seconds 3

  Log "Step 4: Reconnect Warp"
  & $WarpCli connect 2>&1 | Out-Null
  Start-Sleep -Seconds 5

  $newIp = Get-WarpIp
  Log "New Warp IP: $newIp"

  if ($newIp -and $newIp -ne $oldIp) {
    Log "SUCCESS: IP changed from $oldIp to $newIp"
    return $true
  } elseif ($newIp -eq $oldIp) {
    Log "WARNING: IP unchanged ($oldIp), trying Level 3 reset..."
    & $WarpCli disconnect 2>&1 | Out-Null
    Start-Sleep -Seconds 2
    Remove-Item -Path "$DataDir\*" -Include "*.sqlite", "*.json" -Force -ErrorAction SilentlyContinue
    Start-Sleep -Seconds 1
    & $WarpCli register 2>&1 | Out-Null
    Start-Sleep -Seconds 3
    & $WarpCli connect 2>&1 | Out-Null
    Start-Sleep -Seconds 5
    $newIp = Get-WarpIp
    Log "After Level 3 reset - New IP: $newIp"
    if ($newIp -and $newIp -ne $oldIp) {
      Log "SUCCESS: IP changed after Level 3 reset: $oldIp -> $newIp"
      return $true
    }
    return $false
  } else {
    Log "ERROR: Warp not reachable after rotation"
    return $false
  }
}

function Clear-NineRouterCooldown {
  Log "Clearing 9Router provider cooldown for opencode..."
  try {
    $settings = Invoke-RestMethod -Uri "$NineRouterUrl/api/settings" -Method Get -TimeoutSec 10
    $strategies = $settings.providerStrategies
    if ($strategies.opencode) {
      $body = @{ providerStrategies = @{ opencode = @{ proxyPoolId = $ProxyPoolId } } } | ConvertTo-Json -Depth 5
      Invoke-RestMethod -Uri "$NineRouterUrl/api/settings" -Method Patch -Body $body -ContentType "application/json" -TimeoutSec 10 | Out-Null
      Log "9Router proxy binding refreshed"
    }
  } catch {
    Log "Warning: Could not refresh 9Router binding: $_"
  }
}

function Check-For429 {
  Log "Checking for 429 errors from opencode provider..."
  try {
    $since = Get-LastErrorTimestamp
    $query = "$NineRouterUrl/api/usage/request-details?provider=opencode&status=error&pageSize=5"
    if ($since) { $query += "&startDate=$since" }

    $result = Invoke-RestMethod -Uri $query -Method Get -TimeoutSec 10
    $latestError = $null

    foreach ($detail in $result.details) {
      if ($detail.response.status -eq 429) {
        $latestError = $detail
        Log "Found 429 at $($detail.timestamp): $($detail.response.error)"
      }
    }

    if ($latestError) {
      $ts = $latestError.timestamp
      $prev = Get-LastErrorTimestamp
      if ($ts -ne $prev) {
        Log "New 429 detected at $ts"
        Save-LastErrorTimestamp $ts
        return $true
      } else {
        Log "429 already handled (last processed: $prev)"
      }
    } else {
      Log "No new 429 errors"
    }
  } catch {
    Log "Warning: Failed to check logs: $_"
  }
  return $false
}

if ($ForceRotate) {
  $ok = Invoke-WarpRotate
  if ($ok) {
    Clear-NineRouterCooldown
    Log "Manual rotation completed"
  } else {
    Log "Manual rotation FAILED"
    exit 1
  }
  exit
}

if ($SkipMonitor) {
  Write-Host "Skipped monitor mode. Run without -SkipMonitor to start monitoring."
  exit
}

Log "=== Warp Rotator Monitor started (poll every ${PollInterval}s) ==="
Log "9Router: $NineRouterUrl"
Log "Warp CLI: $WarpCli"
Log "Proxy Pool ID: $ProxyPoolId"

$PidFile = "$PSScriptRoot\.monitor.pid"
$PID | Out-File -FilePath $PidFile -NoNewline

try {
while ($true) {
  $warpStatus = Get-WarpStatus
  if ($warpStatus -ne "connected") {
    Log "WARNING: Warp status is '$warpStatus', attempting reconnect..."
    & $WarpCli connect 2>&1 | Out-Null
    Start-Sleep -Seconds 5
  }

  if (Check-For429) {
    $ok = Invoke-WarpRotate
    if ($ok) {
      Clear-NineRouterCooldown
      Log "Auto-rotation completed successfully"
    } else {
      Log "CRITICAL: Auto-rotation FAILED"
    }
  }

  Start-Sleep -Seconds $PollInterval
}
} finally {
  Remove-Item "$PSScriptRoot\.monitor.pid" -Force -ErrorAction SilentlyContinue
}
