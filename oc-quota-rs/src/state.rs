use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::{VecDeque, HashMap};
use std::sync::Arc;
use chrono::{DateTime, Utc};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProxyPool {
    pub id: String,
    pub name: String,
    pub url: String,
    pub is_active: bool,
    pub errors: u32,
    pub last_used: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TorInstance {
    pub status: String,
    pub ip: Option<String>,
}

#[derive(Debug)]
pub struct AppState {
    pub proxy_pools: Vec<ProxyPool>,
    pub active_pool_index: usize,
    
    // UI state
    pub active_tab: usize,
    
    // Interceptor stats
    pub total_requests: usize,
    pub total_retries: usize,
    pub last_error: Option<String>,
    pub logs: VecDeque<String>,
    
    // WARP stats
    pub warp_instances: HashMap<u16, TorInstance>, // port -> TorInstance
}

impl AppState {
    pub fn new() -> Self {
        Self {
            proxy_pools: Vec::new(),
            active_pool_index: 0,
            active_tab: 0,
            total_requests: 0,
            total_retries: 0,
            last_error: None,
            logs: VecDeque::with_capacity(50),
            warp_instances: HashMap::new(),
        }
    }

    pub fn log(&mut self, msg: String) {
        if self.logs.len() >= 50 {
            self.logs.pop_front();
        }
        let ts = Utc::now().format("%H:%M:%S").to_string();
        self.logs.push_back(format!("[{}] {}", ts, msg));
    }
}

pub type SharedState = Arc<RwLock<AppState>>;
