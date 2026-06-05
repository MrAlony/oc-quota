# OC-Quota - Multi-Proxy Quota Maximizer for OpenCode Free

Route OpenCode Free traffic through multiple Cloudflare Warp SOCKS5 proxies with automatic IP rotation on 429 rate limits. A smart interceptor proxy ensures OpenCode **never sees a 429** - it handles pool switching internally.

## Architecture

```
OpenCode --> Interceptor (port 20129) --> 9Router (port 20128) --> wireproxy pools --> opencode.ai
                   |                          |
                   |  On 429:                 |
                   |  1. Switch proxy pool    |
                   |  2. Retry internally     |  warp-proxy-1 (port 51001) -- Cloudflare WARP identity #1
                   |  3. Return success       |  warp-proxy-2 (port 51002) -- Cloudflare WARP identity #2
                   |                          |  warp-proxy-3 (port 51003) -- Cloudflare WARP identity #3
                   v                          |  warp-proxy-4 (port 51004) -- Cloudflare WARP identity #4
            OpenCode gets                     |  warp-proxy-5 (port 51005) -- Cloudflare WARP identity #5
            a clean response                  |  (expandable to N instances)
```

- Each wireproxy instance has its own Cloudflare WARP identity and IP
- When one IP gets rate-limited, the interceptor instantly switches to the next
- Only OpenCode Free goes through Warp (per-provider proxy binding)
- Other providers (Atessa, AgentRouter, etc.) stay direct

## Files

| File | Purpose |
|---|---|
| `warp-rotation/multi-proxy.bat` | Setup, start, stop, test N wireproxy SOCKS5 instances |
| `warp-rotation/interceptor.bat` | Smart 429 interceptor proxy - sits between OpenCode and 9Router |
| `warp-rotation/warp-menu.bat` | Interactive menu + background monitor (legacy WARP rotation fallback) |

## Quick Start

```powershell
# 1. Setup: Create 5 WARP proxy identities (one-time)
cd warp-rotation
multi-proxy.bat -Setup -Count 5

# 2. Start all proxy instances
multi-proxy.bat -Start

# 3. Start the 429 interceptor (keeps running)
interceptor.bat

# 4. Point OpenCode to http://localhost:20129 (the interceptor)
```

## How it Works

1. **multi-proxy.bat -Setup** registers N separate Cloudflare WARP accounts using `wgcf`, generates WireGuard configs, and creates `wireproxy` SOCKS5 proxy configs on ports 51001+
2. **multi-proxy.bat -Start** launches all wireproxy instances in the background
3. **interceptor.bat** runs a local HTTP proxy on port 20129 that:
   - Forwards requests to 9Router (port 20128)
   - If 9Router returns **429**: instantly switches the proxy pool, retries internally
   - Only returns 429 to OpenCode if **all pools are exhausted**
   - Bans rate-limited pools for 5 minutes, then auto-unbans
4. **warp-menu.bat** (optional) provides a background monitor that can also detect and handle 429s

## Prerequisites

- **Cloudflare Warp** - [Download](https://1111-repo.cloudflare.com/windows/warp/Cloudflare_WARP_Release-x64.msi) (for legacy rotation fallback)
- **9Router** - Running at `http://localhost:20128`
- Tools auto-downloaded: `wgcf.exe`, `wireproxy.exe` (stored in `warp-rotation/tools/`)
