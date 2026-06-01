$PidFile = "$PSScriptRoot\.monitor.pid"

if (-not (Test-Path $PidFile)) {
  Write-Host "No monitor instance found (no PID file)."
  exit
}

$oldPid = (Get-Content $PidFile -Raw).Trim()
$proc = Get-Process -Id $oldPid -ErrorAction SilentlyContinue

if ($proc) {
  Stop-Process -Id $oldPid -Force
  Write-Host "Killed monitor (PID: $oldPid)"
} else {
  Write-Host "No process with PID $oldPid (stale PID file)"
}

Remove-Item $PidFile -Force -ErrorAction SilentlyContinue
