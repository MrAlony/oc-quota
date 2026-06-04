<# :
@echo off
set "SCRIPT_PATH=%~f0"
powershell.exe -NoProfile -ExecutionPolicy Bypass -Command "& ([ScriptBlock]::Create((Get-Content '%~f0' -Raw)))" %*
exit /b
#>
param(
  [switch]$RunMonitor,
  [switch]$LiveLogs
)

$ScriptRoot = Split-Path $env:SCRIPT_PATH
$NineRouterUrl = "http://localhost:20128"
$WarpCli = "C:\Program Files\Cloudflare\Cloudflare WARP\warp-cli.exe"
$WarpPort = 40000
$ProxyPoolId = "0d130c94-d89a-4e86-a08b-0e3337dede81"
$PidFile = "$ScriptRoot\.monitor.pid"
$StateFile = "$ScriptRoot\.last-error.txt"
$LogPath = "$ScriptRoot\warp-rotator.log"

function Log { 
    param([string]$Msg, [switch]$NoWriteHost) 
    $t = Get-Date -Format "yyyy-MM-dd HH:mm:ss"
    "$t $Msg" | Out-File -FilePath $LogPath -Append
    if (-not $NoWriteHost) { Write-Host "$t $Msg" } 
}

function Get-WarpStatus {
  try {
    $r = & $WarpCli status 2>&1
    if ($r -like "*Connected*") { return "connected" }
    if ($r -like "*Disconnected*") { return "disconnected" }
    if ($r -like "*Connecting*") { return "connecting" }
    Log "Debug Warp status: $r" -NoWriteHost
    return "unknown"
  } catch { return "error" }
}

function Get-WarpIp {
  for ($i = 0; $i -lt 3; $i++) {
    try {
      $ip = (curl.exe --socks5-hostname 127.0.0.1:40000 -m 5 -s https://ifconfig.me/ip 2>$null)
      if ($ip -and $ip -match "^\d+\.\d+\.\d+\.\d+$") { return $ip }
    } catch {}
    Start-Sleep -Seconds 2
  }
  return $null
}

function Get-LastErrorTimestamp {
  if (Test-Path $StateFile) { return (Get-Content $StateFile -Raw).Trim() }
  return ""
}
function Save-LastErrorTimestamp { param([string]$ts) Set-Content -Path $StateFile -Value $ts -NoNewline }

# Invoke-WarpRotate logic moved into Run-MonitorLoop for better ban tracking

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
  if ($LiveLogs) { Log "Checking for 429 errors from opencode provider..." }
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
        break
      }
    }

    if ($latestError) {
      Log "New 429 detected at $($latestError.timestamp)"
      return $true
    } else {
      if ($LiveLogs) { Log "No new 429 errors" }
    }
  } catch {
    Log "Warning: Failed to check logs: $_"
  }
  return $false
}

function Run-MonitorLoop {
  Log "=== Warp Rotator Monitor started (poll every 15s) ==="
  $PID | Out-File -FilePath $PidFile -NoNewline
  $nowIso = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ss.fffZ")
  Save-LastErrorTimestamp $nowIso
  Log "Initialized error checkpoint to $nowIso"
  
  try {
    while ($true) {
      $warpStatus = Get-WarpStatus
      if ($warpStatus -ne "connected") {
        Log "WARNING: Warp status is '$warpStatus', attempting reconnect..."
        & $WarpCli connect 2>&1 | Out-Null
        Start-Sleep -Seconds 5
      }

      if (Check-For429) {
        Log "=== Fast Warp IP Rotation ==="
        & $WarpCli tunnel rotate-keys 2>&1 | Out-Null
        Start-Sleep -Seconds 1
        & $WarpCli disconnect 2>&1 | Out-Null
        Start-Sleep -Seconds 1
        & $WarpCli connect 2>&1 | Out-Null
        Start-Sleep -Seconds 5
        
        $newIp = Get-WarpIp
        if ($newIp) {
          Log "SUCCESS: Acquired IP: $newIp"
        } else {
          Log "WARNING: Could not fetch IP, continuing anyway..."
        }

        Clear-NineRouterCooldown
        $nowIso = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ss.fffZ")
        Save-LastErrorTimestamp $nowIso
        Log "Auto-rotation completed instantly."
      } else {
        if ($LiveLogs) { Log "No new 429 errors" }
      }
      Start-Sleep -Seconds 15
    }
  } finally {
    Remove-Item $PidFile -Force -ErrorAction SilentlyContinue
  }
}

function Stop-Monitor {
  if (-not (Test-Path $PidFile)) {
    return "Not Running"
  }
  $oldPid = (Get-Content $PidFile -Raw).Trim()
  $proc = Get-Process -Id $oldPid -ErrorAction SilentlyContinue
  if ($proc) {
    Stop-Process -Id $oldPid -Force
    Remove-Item $PidFile -Force -ErrorAction SilentlyContinue
    return "Stopped"
  } else {
    Remove-Item $PidFile -Force -ErrorAction SilentlyContinue
    return "Cleaned up stale PID"
  }
}

function Run-Setup {
  Clear-Host
  Write-Host "=== Running Setup ===" -ForegroundColor Cyan
  
  if (-not (Get-Command "curl" -ErrorAction SilentlyContinue)) {
    Write-Host "ERROR: curl not found." -ForegroundColor Red; return
  }
  if (-not (Test-Path $WarpCli)) {
    Write-Host "ERROR: Warp CLI not found at $WarpCli" -ForegroundColor Red
    Write-Host "Install Cloudflare Warp first." -ForegroundColor Yellow
    return
  }
  
  Write-Host "Checking Warp status..."
  $status = & $WarpCli status 2>&1
  if ($status -like "*Connected*") {
    Write-Host "Warp is connected." -ForegroundColor Green
  } else {
    Write-Host "Warp is not connected. Attempting to connect..." -ForegroundColor Yellow
    & $WarpCli connect 2>&1 | Out-Null
    Start-Sleep -Seconds 3
  }
  
  Write-Host "Checking 9Router..."
  try {
    $r = Invoke-RestMethod -Uri "$NineRouterUrl/api/proxy-pools" -Method Get -TimeoutSec 5
    Write-Host "9Router is running." -ForegroundColor Green
  } catch {
    Write-Host "ERROR: Cannot reach 9Router at $NineRouterUrl" -ForegroundColor Red
    return
  }
  
  Write-Host "Creating warp-socks5 proxy pool if needed..."
  try {
    $pool = $r.proxyPools | Where-Object { $_.name -eq "warp-socks5" }
    if ($pool) {
      $poolId = $pool.id
      Write-Host "Proxy pool 'warp-socks5' exists (ID: $poolId)" -ForegroundColor Green
    } else {
      $body = @{ name = "warp-socks5"; proxyUrl = "socks5://127.0.0.1:$WarpPort"; type = "http" } | ConvertTo-Json
      $newPool = Invoke-RestMethod -Uri "$NineRouterUrl/api/proxy-pools" -Method Post -Body $body -ContentType "application/json" -TimeoutSec 10
      $poolId = $newPool.proxyPool.id
      Write-Host "Created proxy pool 'warp-socks5' (ID: $poolId)" -ForegroundColor Green
    }
  } catch {
    Write-Host "ERROR creating proxy pool: $_" -ForegroundColor Red; return
  }
  
  Write-Host "Binding proxy pool to OpenCode provider..."
  try {
    $settings = Invoke-RestMethod -Uri "$NineRouterUrl/api/settings" -Method Get -TimeoutSec 10
    $existingBinding = $settings.providerStrategies.opencode.proxyPoolId
    if ($existingBinding -eq $poolId) {
      Write-Host "OpenCode already bound to warp-socks5." -ForegroundColor Green
    } else {
      $body = @{ providerStrategies = @{ opencode = @{ proxyPoolId = $poolId } } } | ConvertTo-Json -Depth 5
      Invoke-RestMethod -Uri "$NineRouterUrl/api/settings" -Method Patch -Body $body -ContentType "application/json" -TimeoutSec 10 | Out-Null
      Write-Host "Bound warp-socks5 to OpenCode Free." -ForegroundColor Green
    }
  } catch {
    Write-Host "ERROR binding pool: $_" -ForegroundColor Red; return
  }
  
  Write-Host "Testing connection through 9Router -> Warp..."
  try {
    $testBody = @{ model = "oc/deepseek-v4-flash-free"; messages = @(@{ role = "user"; content = "Say OK" }); stream = $false } | ConvertTo-Json -Depth 3
    $testResult = Invoke-RestMethod -Uri "$NineRouterUrl/v1/chat/completions" -Method Post -Body $testBody -ContentType "application/json" -TimeoutSec 60
    Write-Host "Test SUCCESS! Reply: $($testResult.choices[0].message.content)" -ForegroundColor Green
  } catch {
    Write-Host "ERROR: Test request failed: $_" -ForegroundColor Red
  }
}

function Show-Menu {
  while ($true) {
    Clear-Host
    Write-Host "=====================================" -ForegroundColor Cyan
    Write-Host "    Warp IP Rotation Manager         " -ForegroundColor Cyan
    Write-Host "=====================================" -ForegroundColor Cyan
    
    # State detection
    $warpStatus = Get-WarpStatus
    $nineRouterUp = $false
    try { $null = Invoke-RestMethod -Uri "$NineRouterUrl/api/settings" -Method Get -TimeoutSec 10; $nineRouterUp = $true } catch {}
    
    $monitorRunning = Test-Path $PidFile
    
    Write-Host ""
    Write-Host "--- Status ---"
    Write-Host "Warp Client : " -NoNewline; if ($warpStatus -eq "connected") { Write-Host "Connected" -ForegroundColor Green } else { Write-Host $warpStatus -ForegroundColor Red }
    Write-Host "9Router     : " -NoNewline; if ($nineRouterUp) { Write-Host "Running" -ForegroundColor Green } else { Write-Host "Not Reachable" -ForegroundColor Red }
    Write-Host "Monitor     : " -NoNewline; if ($monitorRunning) { Write-Host "Running" -ForegroundColor Green } else { Write-Host "Stopped" -ForegroundColor Yellow }
    
    Write-Host ""
    Write-Host "--- Suggestions ---" -ForegroundColor Yellow
    if (-not $nineRouterUp) {
        Write-Host "> Please start 9Router first!" -ForegroundColor Red
    } elseif ($warpStatus -ne "connected") {
        Write-Host "> Warp is not connected. Run Setup (Option 1) to fix." -ForegroundColor Yellow
    } elseif (-not $monitorRunning) {
        Write-Host "> Setup looks good. You should Start Monitor (Option 2 or 3)." -ForegroundColor Green
    } else {
        Write-Host "> Everything is running fine!" -ForegroundColor Green
    }
    
    Write-Host ""
    Write-Host "1. Run Setup & Test Connection"
    Write-Host "2. Start Monitor (Hidden in background)"
    Write-Host "3. Start Monitor (With Live Logs in this console)"
    Write-Host "4. Stop Monitor"
    Write-Host "5. Force Rotate IP now"
    Write-Host "6. Exit"
    
    $choice = Read-Host "`nEnter choice"
    
    switch ($choice) {
      "1" { Run-Setup }
      "2" {
        if ($monitorRunning) { Stop-Monitor | Out-Null }
        Write-Host "Starting monitor in background..." -ForegroundColor Cyan
        Start-Process cmd.exe -ArgumentList "/c `"$env:SCRIPT_PATH`" -RunMonitor" -WindowStyle Hidden
        Start-Sleep -Seconds 2
      }
      "3" {
        if ($monitorRunning) { Stop-Monitor | Out-Null }
        Write-Host "Starting monitor with live logs (Press Ctrl+C to stop)..." -ForegroundColor Cyan
        Run-MonitorLoop
      }
      "4" {
        $res = Stop-Monitor
        Write-Host "Monitor Stop Result: $res" -ForegroundColor Cyan
      }
      "5" {
        $ok = Invoke-WarpRotate
        if ($ok) { Clear-NineRouterCooldown }
      }
      "6" { exit }
    }
    
    if ($choice -ne "6" -and $choice -ne "3") {
      Write-Host "`nPress Enter to return to menu..."
      Read-Host
    }
  }
}

# Main Execution
if ($RunMonitor) {
  if ($LiveLogs) {
      Run-MonitorLoop
  } else {
      # Running hidden from background
      # Suppress host output since it's background
      $global:ProgressPreference = 'SilentlyContinue'
      Run-MonitorLoop
  }
} else {
  $global:ErrorActionPreference = 'SilentlyContinue'
  Show-Menu
}
