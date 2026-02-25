use std::sync::OnceLock;

/// Ping style enum to track platform-specific timeout handling.
#[derive(Clone, Copy, Debug)]
pub enum PingStyle {
    /// Linux style: timeout is in seconds
    LinuxBsd,
    /// Windows style: timeout is in milliseconds
    Windows,
}

/// Ping command configuration for different platforms.
/// Different systems use different flags:
/// - Linux: `-c 1 -W 1` (W is seconds)
/// - Windows: `-n 1 -w 1000` (w is milliseconds)
/// - macOS: `-c 1 -W 1000` (W is milliseconds, different from Linux!)
#[derive(Clone, Copy)]
pub struct PingConfig {
    pub count_flag: &'static str,
    pub timeout_flag: &'static str,
    pub style: PingStyle,
}

/// Cached ping configuration. None means ping is not available,
/// Some(config) means ping is available with this config.
static PING_CONFIG: OnceLock<Option<PingConfig>> = OnceLock::new();

/// Detect which ping command style works on this system.
pub async fn detect_ping_config_async() -> Option<PingConfig> {
    use std::process::Stdio;

    let candidates = [
        // Linux style: -c count, -W timeout_seconds
        PingConfig {
            count_flag: "-c",
            timeout_flag: "-W",
            style: PingStyle::LinuxBsd,
        },
        // Windows style: -n count, -w timeout_ms
        PingConfig {
            count_flag: "-n",
            timeout_flag: "-w",
            style: PingStyle::Windows,
        },
    ];

    for config in candidates {
        let timeout_value = match config.style {
            PingStyle::LinuxBsd => "1",
            PingStyle::Windows => "1000",
        };
        let result = smol::process::Command::new("ping")
            .args([
                config.count_flag,
                "1",
                config.timeout_flag,
                timeout_value,
                "127.0.0.1",
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await;

        if result.map(|s| s.success()).unwrap_or(false) {
            return Some(config);
        }
    }

    None
}

/// Get the cached ping configuration, initializing if necessary.
pub async fn get_or_init_ping_config() -> Option<PingConfig> {
    if let Some(config) = PING_CONFIG.get() {
        return *config;
    }

    let detected = detect_ping_config_async().await;
    *PING_CONFIG.get_or_init(|| detected)
}

/// Ping a host using ICMP ping command with a configurable timeout.
pub async fn ping_host_with_timeout(host: &str, config: &PingConfig, timeout_secs: u64) -> bool {
    use std::process::Stdio;

    let timeout_value = match config.style {
        PingStyle::LinuxBsd => timeout_secs.to_string(),
        PingStyle::Windows => (timeout_secs * 1000).to_string(),
    };

    log::debug!(
        "[REACHABILITY] Attempting ICMP ping to {} with {}s timeout",
        host,
        timeout_secs
    );

    let output = smol::process::Command::new("ping")
        .args([
            config.count_flag,
            "1",
            config.timeout_flag,
            &timeout_value,
            host,
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await;

    let result = output.map(|status| status.success()).unwrap_or(false);
    log::debug!(
        "[REACHABILITY] ICMP ping to {}: {}",
        host,
        if result { "success" } else { "failed" }
    );
    result
}

/// Ping a host using ICMP ping command.
/// The ping command has a built-in 1-second timeout.
pub async fn ping_host(host: &str, config: &PingConfig) -> bool {
    ping_host_with_timeout(host, config, 1).await
}

/// Check if a host is reachable using ICMP ping with configurable timeout.
///
/// Uses ICMP ping if available (preferred to avoid being flagged as brute-force attempts),
/// returns None if ping is not available on the system.
pub async fn ping_check_with_timeout(host: &str, timeout_secs: u64) -> Option<bool> {
    let config = get_or_init_ping_config().await?;
    Some(ping_host_with_timeout(host, &config, timeout_secs).await)
}

/// Check if a host is reachable using ICMP ping.
///
/// Uses ICMP ping if available (preferred to avoid being flagged as brute-force attempts),
/// returns None if ping is not available on the system.
///
/// The ping command uses a built-in 1-second timeout.
pub async fn ping_check(host: &str) -> Option<bool> {
    let config = get_or_init_ping_config().await?;
    Some(ping_host(host, &config).await)
}

/// Check if a host is reachable with configurable timeout.
///
/// Uses ICMP ping only (to avoid being flagged as brute-force attempts on SSH/Telnet ports).
/// Returns false if ping is not available on the system.
pub async fn check_reachability_with_timeout(host: &str, port: u16, timeout_secs: u64) -> bool {
    log::debug!(
        "[REACHABILITY] Checking {}:{} with {}s timeout",
        host,
        port,
        timeout_secs
    );

    let result = ping_check_with_timeout(host, timeout_secs).await.unwrap_or_else(|| {
        log::debug!("[REACHABILITY] ICMP ping not available, returning unreachable");
        false
    });

    log::debug!(
        "[REACHABILITY] Final result for {}:{}: {}",
        host,
        port,
        result
    );
    result
}

/// Check if a host is reachable.
///
/// Uses ICMP ping only (to avoid being flagged as brute-force attempts on SSH/Telnet ports).
/// Returns false if ping is not available on the system.
///
/// The ping command has a built-in 1-second timeout.
pub async fn check_reachability(host: &str, port: u16) -> bool {
    log::debug!("[REACHABILITY] Checking {}:{}", host, port);

    let result = ping_check(host).await.unwrap_or_else(|| {
        log::debug!("[REACHABILITY] ICMP ping not available, returning unreachable");
        false
    });

    log::debug!(
        "[REACHABILITY] Final result for {}:{}: {}",
        host,
        port,
        result
    );
    result
}
