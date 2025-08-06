#[tokio::main]
async fn main() -> anyhow::Result<()> {
    #[cfg(feature = "tracing")]
    {
        use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
        tracing_subscriber::registry()
            .with(tracing_subscriber::EnvFilter::new(
                std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into()),
            ))
            .with(tracing_subscriber::fmt::layer())
            .with(tracing_tracy::TracyLayer::default())
            .init();
    }
    #[cfg(not(feature = "tracing"))]
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

    let uri = get_uri();
    log::debug!("uri: {}", uri);

    let master = ros_core_rs::core::Master::new(uri.to_string());

    let bind_address = get_bind_address();
    log::debug!("bind_address: {}", bind_address);
    master.serve(bind_address).await
}

fn get_uri() -> String {
    // Get address from ROS_IP/ROS_HOSTNAME env vars first
    let addr = if let Ok(hostname) = std::env::var("ROS_HOSTNAME") {
        Some(hostname)
    } else if let Ok(ip) = std::env::var("ROS_IP") {
        Some(ip)
    } else {
        None
    };

    // If no env vars, try getting hostname
    let addr = addr.or_else(|| {
        hostname::get()
            .ok()
            .and_then(|h| h.to_str().map(|s| s.to_string()))
            .and_then(|hostname| {
                // Only use hostname if it's not localhost/127.0.0.1
                if hostname != "localhost" && !hostname.starts_with("127.") && hostname != "::" {
                    Some(hostname)
                } else {
                    None
                }
            })
    });

    // Fall back to local address if needed
    let addr = addr.unwrap_or_else(|| {
        // TODO: Implement get_local_address() equivalent
        "127.0.0.1".to_string()
    });

    format!("http://{}:11311/", addr)
}

fn get_bind_address() -> std::net::SocketAddr {
    let address = get_address_override();

    let ip_str = if let Some(addr) = address {
        if addr == "localhost" || addr.starts_with("127.") || addr == "::1" {
            // localhost or 127/8
            if use_ipv6() {
                "::1"
            } else if addr.starts_with("127.") {
                &addr
            } else {
                "127.0.0.1" // loopback
            }
        } else {
            if use_ipv6() {
                "::"
            } else {
                "0.0.0.0"
            }
        }
        .to_string()
    } else {
        if use_ipv6() { "::" } else { "0.0.0.0" }.to_string()
    };

    // Parse the IP string into a SocketAddr with port 0
    format!("{}:11311", ip_str).parse().unwrap()
}

fn get_address_override() -> Option<String> {
    // Check command line args first
    for arg in std::env::args() {
        if arg.starts_with("__hostname:=") || arg.starts_with("__ip:=") {
            if let Some(val) = arg.split(":=").nth(1) {
                return Some(val.to_string());
            }
            log::warn!("Invalid ROS command-line remapping argument '{}'", arg);
            return None;
        }
    }

    // Check ROS_HOSTNAME env var
    if let Ok(hostname) = std::env::var("ROS_HOSTNAME") {
        if hostname.is_empty() {
            log::warn!("Invalid ROS_HOSTNAME (an empty string)");
        } else {
            if hostname.contains("://") {
                log::warn!("Invalid ROS_HOSTNAME (protocol should not be included)");
            } else if hostname.contains(':') {
                log::warn!("Invalid ROS_HOSTNAME (port should not be included)");
            }
            return Some(hostname);
        }
    }

    // Check ROS_IP env var
    if let Ok(ip) = std::env::var("ROS_IP") {
        if ip.is_empty() {
            log::warn!("Invalid ROS_IP (an empty string)");
        } else if ip.contains("://") {
            log::warn!("Invalid ROS_IP (protocol should not be included)");
        } else if ip.contains('.')
            && ip
                .rfind(':')
                .map_or(false, |i| i > ip.rfind('.').unwrap_or(0))
        {
            log::warn!("Invalid ROS_IP (port should not be included)");
        } else if !ip.contains('.') && !ip.contains(':') {
            log::warn!("Invalid ROS_IP (must be a valid IPv4 or IPv6 address)");
        } else {
            return Some(ip);
        }
    }

    None
}

fn use_ipv6() -> bool {
    std::env::var("ROS_IPV6").map_or(false, |v| v == "on")
}
