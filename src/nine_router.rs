use crate::state::SharedState;
use std::process::Stdio;
use tokio::process::Command;
use std::env;

async fn find_9router_port(client: &reqwest::Client) -> Option<u16> {
    for port in [20130, 20128] {
        if client.get(format!("http://127.0.0.1:{}/api/proxy-pools", port)).send().await.is_ok() 
        || client.get(format!("http://localhost:{}/api/proxy-pools", port)).send().await.is_ok() {
            return Some(port);
        }
    }
    None
}

pub async fn run_9router(state: SharedState) {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build()
        .unwrap();

    let mut running_port = find_9router_port(&client).await;
    
    if let Some(port) = running_port {
        let mut s = state.write();
        s.log(format!("9Router is already running on port {}. Skipping launch.", port));
    } else {
        {
            let mut s = state.write();
            s.log("Starting 9Router GUI...".to_string());
        }

        let cmd_path = "C:\\Users\\dell\\AppData\\Roaming\\npm\\9router.cmd";

        #[cfg(windows)]
        use std::os::windows::process::CommandExt;
        
        let mut command = Command::new("cmd");
        command.arg("/c")
               .arg(cmd_path)
               .arg("--tray")
               .arg("--no-browser")
               .stdout(Stdio::null())
               .stderr(Stdio::null());

        #[cfg(windows)]
        command.creation_flags(0x00000008); // DETACHED_PROCESS

        let child = command.spawn();

        match child {
            Err(e) => {
                let mut s = state.write();
                s.log(format!("Failed to start 9Router: {}", e));
            }
            Ok(mut c) => {
                tokio::spawn(async move {
                    let _ = c.wait().await;
                });
                
                // Wait and find the port it started on
                for _ in 0..15 {
                    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                    if let Some(p) = find_9router_port(&client).await {
                        running_port = Some(p);
                        break;
                    }
                }
            }
        }
    }

    if let Some(port) = running_port {
        configure_9router(state.clone(), port).await;
    } else {
        let mut s = state.write();
        s.log("Warning: 9Router failed to start or didn't respond in time.".to_string());
    }
}

async fn configure_9router(state: SharedState, port: u16) {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()
        .unwrap();
    
    let base_url = format!("http://127.0.0.1:{}", port);
    
    // 1. Get all proxy pools
    let mut pool_id = None;
    let mut needs_update = false;

    if let Ok(res) = client.get(format!("{}/api/proxy-pools", base_url)).send().await {
        if let Ok(pools) = res.json::<serde_json::Value>().await {
            if let Some(arr) = pools["proxyPools"].as_array() {
                for p in arr {
                    if p["name"] == "OC-Quota-Rust-Interceptor" {
                        pool_id = p["id"].as_str().map(|s| s.to_string());
                        if p["proxyUrl"] != "http://127.0.0.1:20131" {
                            needs_update = true;
                        }
                        break;
                    }
                }
            }
        }
    }

    // 2. If it exists but wrong URL, delete it so we can recreate it properly
    if needs_update {
        if let Some(ref id) = pool_id {
            let _ = client.delete(format!("{}/api/proxy-pools/{}", base_url, id)).send().await;
            pool_id = None; // Reset so we create a fresh one
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        }
    }

    // 3. Create it if it doesn't exist
    if pool_id.is_none() {
        let body = serde_json::json!({
            "name": "OC-Quota-Rust-Interceptor",
            "proxyUrl": "http://127.0.0.1:20131",
            "type": "http",
            "isActive": true
        });
        if let Ok(res) = client.post(format!("{}/api/proxy-pools", base_url)).json(&body).send().await {
            if let Ok(p) = res.json::<serde_json::Value>().await {
                pool_id = p["id"].as_str().map(|s| s.to_string());
            }
        }
    }

    // 4. Configure the opencode strategy
    let body = serde_json::json!({
        "providerStrategies": {
            "opencode": {
                "baseUrl": "http://127.0.0.1:20131",
                "proxyPoolId": pool_id
            }
        }
    });

    let res = client
        .patch(format!("{}/api/settings", base_url))
        .json(&body)
        .send()
        .await;

    match res {
        Ok(r) if r.status().is_success() => {
            let mut s = state.write();
            s.log(format!("Auto-configured 9Router (Port {}) with Rust Proxy Pool!", port));
        }
        _ => {
            let mut s = state.write();
            s.log("Warning: Failed to auto-configure 9Router settings.".to_string());
        }
    }
}
