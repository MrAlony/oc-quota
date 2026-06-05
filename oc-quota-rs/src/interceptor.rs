use crate::state::SharedState;
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use reqwest::Client;
use std::convert::Infallible;
use std::net::SocketAddr;
use tokio::net::TcpListener;

const UPSTREAM: &str = "https://api.minimax.chat";

pub async fn run_interceptor(state: SharedState) {
    let addr = SocketAddr::from(([127, 0, 0, 1], 20131));
    let listener = match TcpListener::bind(addr).await {
        Ok(l) => {
            let mut s = state.write();
            s.log("Unified Interceptor listening on 127.0.0.1:20131".to_string());
            l
        }
        Err(e) => {
            let mut s = state.write();
            s.log(format!("Failed to bind interceptor port: {}", e));
            return;
        }
    };

    loop {
        if let Ok((stream, _)) = listener.accept().await {
            let io = TokioIo::new(stream);
            let state_clone = state.clone();

            tokio::task::spawn(async move {
                if let Err(_) = http1::Builder::new()
                    .preserve_header_case(true)
                    .title_case_headers(true)
                    .serve_connection(io, service_fn(move |req| handle_request(req, state_clone.clone())))
                    .with_upgrades()
                    .await
                {
                    // Ignore connection errors silently to avoid spam
                }
            });
        }
    }
}

async fn handle_request(
    req: Request<hyper::body::Incoming>,
    state: SharedState,
) -> Result<Response<http_body_util::combinators::BoxBody<Bytes, std::io::Error>>, Infallible> {
    let is_completion = req.uri().path().contains("/completions") || req.uri().path().contains("/chat/completions");
    
    let method = req.method().clone();
    let uri = req.uri().clone();
    let headers = req.headers().clone();
    
    println!("DEBUG: Incoming request: {} {}", method, uri);

    if method == hyper::Method::CONNECT {
        if let Some(addr) = req.uri().authority().map(|auth| auth.to_string()) {
            tokio::task::spawn(async move {
                match hyper::upgrade::on(req).await {
                    Ok(upgraded) => {
                        println!("DEBUG: Upgrade successful for {}", addr);
                        let mut io = hyper_util::rt::TokioIo::new(upgraded);
                        if let Ok(mut server) = tokio::net::TcpStream::connect(addr.clone()).await {
                            println!("DEBUG: Connected to target TCP server for {}", addr);
                            match tokio::io::copy_bidirectional(&mut io, &mut server).await {
                                Ok((to_server, to_client)) => {
                                    println!("DEBUG: Tunnel closed for {} ({} bytes up, {} bytes down)", addr, to_server, to_client);
                                }
                                Err(e) => {
                                    println!("DEBUG: Tunnel error for {}: {}", addr, e);
                                }
                            }
                        } else {
                            println!("DEBUG: Failed to connect to target TCP server for {}", addr);
                        }
                    }
                    Err(e) => {
                        println!("DEBUG: Upgrade failed: {}", e);
                    }
                }
            });
            let empty_body = http_body_util::Empty::new().map_err(|never| match never {}).boxed();
            return Ok(Response::builder().status(StatusCode::OK).body(empty_body).unwrap());
        } else {
            let empty_body = http_body_util::Full::new(Bytes::new()).map_err(|never| match never {}).boxed();
            return Ok(Response::builder().status(StatusCode::BAD_REQUEST).body(empty_body).unwrap());
        }
    }

    // Read full body for the request (so we can retry it)
    let body_bytes = match req.collect().await {
        Ok(b) => b.to_bytes(),
        Err(_) => Bytes::new(),
    };

    let get_active_pool = || -> Option<crate::state::ProxyPool> {
        let s = state.read();
        if s.proxy_pools.is_empty() {
            None
        } else {
            Some(s.proxy_pools[s.active_pool_index].clone())
        }
    };

    if !is_completion {
        // Passthrough directly
        let pool = get_active_pool();
        let (status, res_headers, body_stream) = forward_request_stream(&method, &uri, &headers, body_bytes.clone(), pool).await;
        let mut hyper_res = Response::new(body_stream);
        *hyper_res.status_mut() = status;
        *hyper_res.headers_mut() = res_headers;
        return Ok(hyper_res);
    }

    let mut attempt = 0;
    loop {
        attempt += 1;
        {
            let mut s = state.write();
            s.total_requests += 1;
        }

        let pool = get_active_pool();
        let (status, mut res_headers, body_stream) = forward_request_stream(&method, &uri, &headers, body_bytes.clone(), pool).await;
        
        if status != StatusCode::TOO_MANY_REQUESTS && status.as_u16() < 500 {
            let mut s = state.write();
            if attempt > 1 {
                s.log(format!("OK: Request succeeded on attempt {}", attempt));
            } else {
                s.log("OK: Request succeeded on first try".to_string());
            }
            
            let mut hyper_res = Response::new(body_stream);
            *hyper_res.status_mut() = status;
            *hyper_res.headers_mut() = res_headers;
            return Ok(hyper_res);
        }

        {
            let mut s = state.write();
            s.total_retries += 1;
            s.log(format!("FAIL ({}) on attempt {} - switching pool...", status, attempt));
        }

        if !switch_pool(state.clone()).await {
            {
                let mut s = state.write();
                s.log("CRITICAL: Failed to switch pool. Waiting 2s...".to_string());
            }
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }
    }
}

async fn forward_request_stream(
    method: &hyper::Method,
    uri: &hyper::Uri,
    headers: &hyper::HeaderMap,
    body: Bytes,
    proxy_pool: Option<crate::state::ProxyPool>,
) -> (StatusCode, hyper::HeaderMap, http_body_util::combinators::BoxBody<Bytes, std::io::Error>) {
    let mut builder = Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(300)); // 5 minutes max for full stream

    if let Some(pool) = proxy_pool {
        if !pool.url.is_empty() {
            if let Ok(proxy) = reqwest::Proxy::all(&pool.url) {
                builder = builder.proxy(proxy);
            }
        }
    }

    let client = builder.build().unwrap();

    let target_url = if let Some(host) = uri.host() {
        let mut scheme = uri.scheme_str().unwrap_or("http");
        if host.contains("api.minimax.chat") {
            scheme = "https";
        }
        format!("{}://{}{}", scheme, host, uri.path_and_query().map(|x| x.as_str()).unwrap_or(""))
    } else {
        format!("{}{}", UPSTREAM, uri.path_and_query().map(|x| x.as_str()).unwrap_or(""))
    };
    
    let mut req_builder = client.request(method.clone(), target_url);
    for (k, v) in headers.iter() {
        if k.as_str().to_lowercase() != "host" {
            req_builder = req_builder.header(k, v);
        }
    }
    req_builder = req_builder.body(body);

    match req_builder.send().await {
        Ok(res) => {
            let status = res.status();
            let mut response_headers = hyper::HeaderMap::new();
            for (k, v) in res.headers().iter() {
                // Remove Transfer-Encoding and Content-Encoding to avoid breaking hyper's chunked output
                let key_str = k.as_str().to_lowercase();
                if key_str != "transfer-encoding" && key_str != "content-encoding" {
                    response_headers.insert(k.clone(), v.clone());
                }
            }
            
            use futures_util::StreamExt;
            use http_body_util::StreamBody;
            use hyper::body::Frame;

            let stream = res.bytes_stream().map(|result| {
                match result {
                    Ok(bytes) => Ok(Frame::data(bytes)),
                    Err(e) => Err(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())),
                }
            });
            let stream_body = StreamBody::new(stream);
            let box_body = http_body_util::BodyExt::boxed(stream_body);
            
            (status, response_headers, box_body)
        }
        Err(e) => {
            let error_msg = Bytes::from(format!("Proxy error: {}", e));
            let stream_body = http_body_util::Full::new(error_msg).map_err(|never| match never {}).boxed();
            (StatusCode::BAD_GATEWAY, hyper::HeaderMap::new(), stream_body)
        }
    }
}

async fn switch_pool(state: SharedState) -> bool {
    let mut tor_num: Option<u16> = None;
    
    let (next_name, next_index, total_pools) = {
        let mut s = state.write();
        
        if s.proxy_pools.is_empty() {
            return false;
        }

        let active_index = s.active_pool_index;
        let active_id = s.proxy_pools[active_index].id.clone();
        
        if active_id.starts_with("tor-") {
            let num_str = active_id.trim_start_matches("tor-");
            if let Ok(num) = num_str.parse::<u16>() {
                tor_num = Some(num);
                s.log(format!("Tor-{} hit Rate Limit! Failing over...", num));
                
                // Clear the IP so the dashboard shows it's waiting for a new one
                let socks_port = 9050 + num;
                if let Some(instance) = s.warp_instances.get_mut(&socks_port) {
                    instance.ip = None;
                    instance.status = format!("Rotating IP (Control {})", 9060 + num);
                }
            }
        }

        // Always instantly failover to the next proxy
        s.active_pool_index = (active_index + 1) % s.proxy_pools.len();
        
        let next = &s.proxy_pools[s.active_pool_index];
        (next.name.clone(), s.active_pool_index, s.proxy_pools.len())
    };

    if let Some(num) = tor_num {
        let state_clone = state.clone();
        tokio::spawn(async move {
            let control_port = 9060 + num;
            if crate::tor::rotate_tor_ip(control_port).await {
                let mut s = state_clone.write();
                s.log(format!("Tor-{} successfully acquired new Identity!", num));
            } else {
                let mut s = state_clone.write();
                s.log(format!("Tor-{} failed to rotate IP.", num));
            }
        });
    }

    {
        let mut s = state.write();
        s.log(format!("SWITCH: Now using {} (active: {}/{})", next_name, next_index + 1, total_pools));
    }
    
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    true
}
