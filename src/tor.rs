use crate::state::SharedState;
use tokio::process::Command;
use std::process::Stdio;
use std::time::Duration;
use tokio::time::sleep;
use tokio::net::TcpStream;
use tokio::io::AsyncWriteExt;
use std::path::Path;
use std::env;

pub async fn run_tor_manager(state: SharedState) {
    let current_dir = env::current_dir().expect("Failed to get current directory");
    let tools_dir = format!("{}\\tools", current_dir.display());
    let tor_exe = format!("{}\\tor\\tor.exe", tools_dir);
    let instances_dir = format!("{}\\instances", current_dir.display());

    {
        let mut s = state.write();
        s.log("Tor Manager starting...".to_string());
    }

    // Auto-download Tor Expert Bundle if not present
    if !Path::new(&tor_exe).exists() {
        {
            let mut s = state.write();
            s.log("tor.exe not found! Downloading Tor Expert Bundle (this may take a minute)...".to_string());
        }
        
        let download_url = "https://dist.torproject.org/torbrowser/15.0.15/tor-expert-bundle-windows-x86_64-15.0.15.tar.gz";
        let archive_path = format!("{}\\tor.tar.gz", tools_dir);
        
        std::fs::create_dir_all(&tools_dir).unwrap_or_default();

        let client = reqwest::Client::new();
        if let Ok(mut resp) = client.get(download_url).send().await {
            use futures_util::StreamExt;
            let mut file = tokio::fs::File::create(&archive_path).await.unwrap();
            while let Some(chunk) = resp.chunk().await.unwrap_or(None) {
                tokio::io::AsyncWriteExt::write_all(&mut file, &chunk).await.unwrap();
            }
            
            {
                let mut s = state.write();
                s.log("Extracting Tor Bundle...".to_string());
            }

            // Extract using tar
            let _ = Command::new("tar")
                .arg("-xf")
                .arg(&archive_path)
                .arg("-C")
                .arg(&tools_dir)
                .spawn()
                .unwrap()
                .wait()
                .await;

            std::fs::remove_file(archive_path).unwrap_or_default();
            
            {
                let mut s = state.write();
                s.log("Tor Bundle ready.".to_string());
            }
        } else {
            let mut s = state.write();
            s.log("Failed to download Tor!".to_string());
            return;
        }
    }

    std::fs::create_dir_all(&instances_dir).unwrap_or_default();

    // Spawn 5 Tor instances
    for i in 1..=5 {
        let socks_port = 9050 + i;
        let control_port = socks_port + 10;
        let instance_path = format!("{}\\tor{}", instances_dir, i);
        let torrc_path = format!("{}\\torrc", instance_path);

        std::fs::create_dir_all(&instance_path).unwrap_or_default();

        // Write torrc
        let torrc_content = format!(
            "SocksPort {}\nControlPort {}\nDataDirectory {}\nExitNodes {{US}},{{UK}},{{CA}},{{DE}},{{FR}}\nStrictNodes 0\n",
            socks_port, control_port, instance_path
        );
        std::fs::write(&torrc_path, torrc_content).unwrap();

        {
            let mut s = state.write();
            s.warp_instances.insert(socks_port, crate::state::TorInstance {
                status: format!("Starting (Control {})", control_port),
                ip: None,
            });
        }
        
        let exe = tor_exe.clone();
        tokio::spawn(async move {
            #[cfg(windows)]
            use std::os::windows::process::CommandExt;

            let mut command = Command::new(&exe);
            command.arg("-f")
                   .arg(&torrc_path)
                   .stdout(Stdio::null())
                   .stderr(Stdio::null());
            
            #[cfg(windows)]
            command.creation_flags(0x08000000); // CREATE_NO_WINDOW
            
            let child = command.spawn();

            match child {
                Ok(mut c) => {
                    let _ = c.wait().await;
                }
                Err(_) => {}
            }
        });

        // Register it as a proxy pool
        let proxy_url = format!("socks5h://127.0.0.1:{}", socks_port);
        {
            let mut s = state.write();
            let pool = crate::state::ProxyPool {
                id: format!("tor-{}", i),
                name: format!("Tor #{}", i),
                url: proxy_url.clone(),
                is_active: true,
                errors: 0,
                last_used: None,
            };
            s.proxy_pools.push(pool);
        }

        // Spawn IP fetching task
        let state_clone = state.clone();
        tokio::spawn(async move {
            let proxy = reqwest::Proxy::all(&proxy_url).unwrap();
            let client = reqwest::Client::builder().proxy(proxy).build().unwrap();

            loop {
                match client.get("https://api.ipify.org").send().await {
                    Ok(resp) => {
                        if let Ok(ip) = resp.text().await {
                            let mut s = state_clone.write();
                            if let Some(instance) = s.warp_instances.get_mut(&socks_port) {
                                instance.status = format!("Running (Control {})", control_port);
                                instance.ip = Some(ip.trim().to_string());
                            }
                            s.log(format!("Tor-{} ready. Exit Node IP: {}", socks_port, ip.trim()));
                            break;
                        }
                    }
                    Err(_) => {}
                }
                tokio::time::sleep(Duration::from_secs(3)).await;
            }
        });
    }
}

pub async fn rotate_tor_ip(control_port: u16) -> bool {
    let target = format!("127.0.0.1:{}", control_port);
    if let Ok(mut stream) = TcpStream::connect(target).await {
        let auth_cmd = b"AUTHENTICATE\r\n";
        let _ = stream.write_all(auth_cmd).await;
        // Read response ideally, but let's blindly send NEWNYM
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        let newnym_cmd = b"SIGNAL NEWNYM\r\n";
        if stream.write_all(newnym_cmd).await.is_ok() {
            let _ = stream.write_all(b"QUIT\r\n").await;
            return true;
        }
    }
    false
}
