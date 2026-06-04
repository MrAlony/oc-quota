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
| `warp-menu.bat` | All-in-one interactive menu to setup, monitor, and manually control the Warp IP rotation |

## Usage

```powershell
# Open the interactive menu
.\warp-menu.bat

# From the menu, you can:
# 1. Run Setup (creates pool, binds OpenCode Free, tests the connection)
# 2. Start Monitor in the background (hidden)
# 3. Start Monitor with live logs
# 4. Stop Monitor
# 5. Force Rotate IP manually
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
