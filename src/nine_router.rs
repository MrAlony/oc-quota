use crate::state::SharedState;
use std::process::Stdio;
use tokio::process::Command;
use std::env;

pub async fn run_9router(state: SharedState) {
    // 1. Check if 9Router is already running
    let is_running = tokio::net::TcpStream::connect("127.0.0.1:20128").await.is_ok();
    
    if is_running {
        {
            let mut s = state.write();
            s.log("9Router is already running. Skipping launch.".to_string());
        }
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
               .env("PORT", "20128")
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
                // Wait a moment for it to start
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
            }
        }
    }

    // Configure 9Router to use our Custom Base URL!
    configure_9router(state.clone()).await;
}

async fn configure_9router(state: SharedState) {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()
        .unwrap();
    
    // Wait for 9router to become available
    let mut is_up = false;
    for _ in 0..15 {
        if client.get("http://127.0.0.1:20128/api/proxy-pools").send().await.is_ok() {
            is_up = true;
            break;
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    }

    if !is_up {
        let mut s = state.write();
        s.log("Warning: 9Router failed to start or didn't respond in time.".to_string());
        return;
    }

    // 1. Get all proxy pools to see if ours exists
    let mut pool_id = None;
    if let Ok(res) = client.get("http://127.0.0.1:20128/api/proxy-pools").send().await {
        if let Ok(pools) = res.json::<serde_json::Value>().await {
            if let Some(arr) = pools["proxyPools"].as_array() {
                for p in arr {
                    if p["name"] == "OC-Quota-Rust-Interceptor" {
                        pool_id = p["id"].as_str().map(|s| s.to_string());
                        break;
                    }
                }
            }
        }
    }

    // 2. If it doesn't exist, create it!
    if pool_id.is_none() {
        let body = serde_json::json!({
            "name": "OC-Quota-Rust-Interceptor",
            "proxyUrl": "http://127.0.0.1:20131",
            "type": "http",
            "isActive": true
        });
        if let Ok(res) = client.post("http://127.0.0.1:20128/api/proxy-pools").json(&body).send().await {
            if let Ok(p) = res.json::<serde_json::Value>().await {
                pool_id = p["id"].as_str().map(|s| s.to_string());
            }
        }
    }

    // 3. Configure the opencode strategy to use BOTH the baseUrl (for unencrypted interception) AND the proxyPoolId (for the UI)
    let body = serde_json::json!({
        "providerStrategies": {
            "opencode": {
                "baseUrl": "http://127.0.0.1:20131",
                "proxyPoolId": pool_id
            }
        }
    });

    let res = client
        .patch("http://127.0.0.1:20128/api/settings")
        .json(&body)
        .send()
        .await;

    match res {
        Ok(r) if r.status().is_success() => {
            let mut s = state.write();
            s.log("Auto-configured 9Router with Rust Proxy Pool!".to_string());
        }
        _ => {
            let mut s = state.write();
            s.log("Warning: Failed to auto-configure 9Router settings.".to_string());
        }
    }
}
