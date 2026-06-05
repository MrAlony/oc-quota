use crate::state::SharedState;
use std::process::Command;
use std::time::Duration;
use tokio::time::sleep;

pub async fn run_warp_manager(state: SharedState) {
    // Check if wireproxy is installed in the tools directory
    let tools_dir = "c:\\Users\\dell\\Desktop\\mralony\\projects\\oc-quota\\warp-rotation\\tools";
    let wireproxy_exe = format!("{}\\wireproxy.exe", tools_dir);
    let instances_dir = "c:\\Users\\dell\\Desktop\\mralony\\projects\\oc-quota\\warp-rotation\\instances";

    {
        let mut s = state.write();
        s.log("WARP Manager started. Initializing instances...".to_string());
    }

    // In a full implementation, we would spawn Command::new(wireproxy_exe)
    // for each warp-X.conf in the instances directory.
    // For now, we simulate monitoring the existing ones, or we can just spawn them.
    for i in 1..=5 {
        let port = 51000 + i;
        {
            let mut s = state.write();
            s.warp_instances.insert(port, "Starting...".to_string());
        }
        
        let conf_path = format!("{}\\warp-{}.conf", instances_dir, i);
        let exe = wireproxy_exe.clone();
        
        tokio::spawn(async move {
            // Wait for existing ones to stop or just simulate for now
            // Actually spawn the process in the background
            /*
            let mut child = Command::new(&exe)
                .arg("-c")
                .arg(&conf_path)
                .spawn()
                .expect("Failed to start wireproxy");
            */
            // Since start-all.bat already runs them, we'll just mark them active in this POC
            loop {
                sleep(Duration::from_secs(10)).await;
            }
        });

        {
            let mut s = state.write();
            s.warp_instances.insert(port, "Running".to_string());
        }
    }
}
