param(
  [string]$NineRouterUrl = "http://localhost:20128",
  [int]$WarpPort = 40000,
  [switch]$SkipMonitor
)

$WarpCli = "C:\Program Files\Cloudflare\Cloudflare WARP\warp-cli.exe"
$LogPath = "$PSScriptRoot\warp-setup.log"

function Log { param([string]$Msg) $t = Get-Date -Format "yyyy-MM-dd HH:mm:ss"; "$t $Msg" | Out-File -FilePath $LogPath -Append; Write-Host "$t $Msg" }
function Step { param([string]$Msg) Write-Host "`n>>> $Msg" -ForegroundColor Cyan; Log ">>> $Msg" }

function Check-Command($cmd, $label) {
  if (-not (Get-Command $cmd -ErrorAction SilentlyContinue)) {
    Write-Host "ERROR: $label not found. Please install it first." -ForegroundColor Red
    exit 1
  }
}

Step "Step 1: Verifying prerequisites"

Check-Command "powershell" "PowerShell"
Check-Command "curl" "curl"

if (-not (Test-Path $WarpCli)) {
  Write-Host "ERROR: Warp CLI not found at $WarpCli" -ForegroundColor Red
  Write-Host "Install Cloudflare Warp from https://1111-repo.cloudflare.com/windows/warp/Cloudflare_WARP_Release-x64.msi"
  Write-Host "Then enable 'Proxy' mode via: $WarpCli mode proxy; $WarpCli proxy port $WarpPort; $WarpCli connect"
  exit 1
}
Log "Warp CLI found: $WarpCli"

$status = & $WarpCli status 2>&1
if ($status -like "*Connected*") {
  Log "Warp is connected"
} else {
  Write-Host "WARNING: Warp status is not 'Connected'. Attempting to connect..." -ForegroundColor Yellow
  & $WarpCli connect 2>&1 | Out-Null
  Start-Sleep -Seconds 3
}

Step "Step 2: Checking 9Router is running"
try {
  $r = Invoke-RestMethod -Uri "$NineRouterUrl/api/proxy-pools" -Method Get -TimeoutSec 5
  Log "9Router is running at $NineRouterUrl"
} catch {
  Write-Host "ERROR: Cannot reach 9Router at $NineRouterUrl" -ForegroundColor Red
  Write-Host "Make sure 9Router is running before running this script."
  exit 1
}

Step "Step 3: Creating warp-socks5 proxy pool"
try {
  $existing = Invoke-RestMethod -Uri "$NineRouterUrl/api/proxy-pools" -Method Get -TimeoutSec 5
  $pool = $existing.proxyPools | Where-Object { $_.name -eq "warp-socks5" }
  if ($pool) {
    $poolId = $pool.id
    Log "Proxy pool 'warp-socks5' already exists (ID: $poolId)"
  } else {
    $body = @{
      name = "warp-socks5"
      proxyUrl = "socks5://127.0.0.1:$WarpPort"
      type = "http"
    } | ConvertTo-Json
    $newPool = Invoke-RestMethod -Uri "$NineRouterUrl/api/proxy-pools" -Method Post -Body $body -ContentType "application/json" -TimeoutSec 10
    $poolId = $newPool.proxyPool.id
    Log "Created proxy pool 'warp-socks5' (ID: $poolId)"
  }
} catch {
  Write-Host "ERROR creating proxy pool: $_" -ForegroundColor Red
  exit 1
}

Step "Step 4: Binding proxy pool to OpenCode Free provider"
try {
  $settings = Invoke-RestMethod -Uri "$NineRouterUrl/api/settings" -Method Get -TimeoutSec 10
  $existingBinding = $settings.providerStrategies.opencode.proxyPoolId

  if ($existingBinding -eq $poolId) {
    Log "OpenCode Free already bound to warp-socks5 (ID: $poolId)"
  } else {
    $body = @{ providerStrategies = @{ opencode = @{ proxyPoolId = $poolId } } } | ConvertTo-Json -Depth 5
    $result = Invoke-RestMethod -Uri "$NineRouterUrl/api/settings" -Method Patch -Body $body -ContentType "application/json" -TimeoutSec 10
    $saved = $result.providerStrategies.opencode.proxyPoolId
    if ($saved -eq $poolId) {
      Log "Bound warp-socks5 to OpenCode Free (ID: $poolId)"
    } else {
      Write-Host "ERROR: Failed to bind proxy pool" -ForegroundColor Red
      exit 1
    }
  }
} catch {
  Write-Host "ERROR: $_" -ForegroundColor Red
  exit 1
}

Step "Step 5: Testing full chain through 9Router -> Warp -> opencode.ai"
try {
  $testModel = "oc/deepseek-v4-flash-free"
  $testBody = @{
    model = $testModel
    messages = @(@{ role = "user"; content = "Say OK" })
    stream = $false
  } | ConvertTo-Json -Depth 3

  $testResult = Invoke-RestMethod -Uri "$NineRouterUrl/v1/chat/completions" -Method Post -Body $testBody -ContentType "application/json" -TimeoutSec 60
  $reply = $testResult.choices[0].message.content
  Log "Test request SUCCESS. Model: $($testResult.model), Reply: $reply"

  $warpIp = curl.exe --socks5-hostname 127.0.0.1:$WarpPort -s https://ifconfig.me/ip 2>$null
  Log "Traffic routed through Warp IP: $warpIp"
} catch {
  $msg = $_.Exception.Message
  Write-Host "ERROR: Test request failed: $msg" -ForegroundColor Red
  Write-Host "Check that Warp is connected and proxy mode is on port $WarpPort" -ForegroundColor Yellow
  exit 1
}

Step "=== SETUP COMPLETE ==="
Write-Host "OpenCode Free is now routed through Cloudflare Warp (SOCKS5 proxy)." -ForegroundColor Green
Write-Host "Traffic goes: your app -> 9Router -> Warp ($warpIp) -> opencode.ai" -ForegroundColor Green

if (-not $SkipMonitor) {
  Write-Host "`nStarting warp-rotator monitor..." -ForegroundColor Yellow
  & "PowerShell" -ExecutionPolicy Bypass -File "$PSScriptRoot\warp-rotator.ps1"
} else {
  Write-Host "`nTo start monitoring later: .\warp-rotator.ps1" -ForegroundColor Yellow
  Write-Host "  (or run warp-setup.ps1 without -SkipMonitor)" -ForegroundColor Yellow
}
