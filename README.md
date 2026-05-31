# OC-Quota — 9Router + Cloudflare Warp IP Rotation

Route OpenCode Free traffic through Cloudflare Warp SOCKS5 proxy with automatic IP rotation on 429 rate limits.

## What it solves

OpenCode Free provider rate-limits by IP. This setup routes all OpenCode Free traffic through Cloudflare Warp (free, unlimited data) and automatically rotates the Warp IP when a 429 is detected, giving you a fresh IP instantly.

## Architecture

```
your app → 9Router → Cloudflare Warp (SOCKS5) → opencode.ai
```

- Only OpenCode Free goes through Warp (per-provider proxy binding)
- Other providers (Atessa, AgentRouter, etc.) stay direct

## Files

| File | Purpose |
|---|---|
| `warp-setup.ps1` | One-time setup — verifies prereqs, creates proxy pool, binds to OpenCode Free, tests the chain |
| `warp-rotator.ps1` | Polls 9Router for 429 errors from OpenCode Free, auto-rotates Warp IP on detection |

## Usage

```powershell
# First time setup (creates pool, binds, starts monitor)
.\warp-setup.ps1

# After PC restart
.\warp-setup.ps1

# Just start monitoring (already set up)
.\warp-rotator.ps1

# Manual IP rotation
.\warp-rotator.ps1 -ForceRotate

# Setup without starting monitor
.\warp-setup.ps1 -SkipMonitor
```

## How rotation works

1. `warp-cli disconnect`
2. Clear Warp registration partials (forces new IP on re-register)
3. `warp-cli register`
4. `warp-cli connect`
5. Refresh 9Router proxy binding
6. If IP unchanged → Level 3 reset (delete all Warp data, full re-register)

## Prerequisites

- **Cloudflare Warp** — [Download](https://1111-repo.cloudflare.com/windows/warp/Cloudflare_WARP_Release-x64.msi)
- **9Router** — Running at `http://localhost:20128`
- **Warp Proxy Mode** — `warp-cli mode proxy` + `warp-cli proxy port 40000` + `warp-cli connect`
