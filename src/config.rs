use std::env;
use std::time::Duration;

/// Server configuration
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// Host to bind to
    pub host: String,
    /// Port to listen on
    pub port: u16,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: env::var("SERVER_HOST").unwrap_or_else(|_| "0.0.0.0".to_string()),
            port: env::var("SERVER_PORT")
                .ok()
                .and_then(|p| p.parse().ok())
                .or_else(|| env::var("PORT").ok().and_then(|p| p.parse().ok()))
                .unwrap_or(8080),
        }
    }
}

impl ServerConfig {
    /// Get the socket address string
    pub fn addr(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

/// Broadcast channel configuration
#[derive(Debug, Clone)]
pub struct BroadcastConfig {
    /// Capacity of the broadcast channel
    pub channel_capacity: usize,
}

impl Default for BroadcastConfig {
    fn default() -> Self {
        Self {
            channel_capacity: env::var("BROADCAST_CAPACITY")
                .ok()
                .and_then(|c| c.parse().ok())
                .unwrap_or(1000),
        }
    }
}

/// Health monitoring configuration
#[derive(Debug, Clone)]
pub struct HealthCheckConfig {
    /// Interval between health checks
    pub check_interval: Duration,
}

impl Default for HealthCheckConfig {
    fn default() -> Self {
        Self {
            check_interval: Duration::from_secs(
                env::var("HEALTH_CHECK_INTERVAL_SECS")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(60),
            ),
        }
    }
}

/// Reconnection configuration for WebSocket streams
#[derive(Debug, Clone)]
pub struct ReconnectConfig {
    /// Maximum number of reconnection attempts
    pub max_retries: u32,
    /// Base backoff duration
    pub base_backoff: Duration,
    /// Maximum backoff duration
    pub max_backoff: Duration,
    /// Stale filter timeout - force reconnect if no events within this duration
    pub stale_timeout: Duration,
}

impl Default for ReconnectConfig {
    fn default() -> Self {
        let base_secs = env::var("RECONNECT_BASE_BACKOFF_SECS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(1);

        Self {
            max_retries: env::var("RECONNECT_MAX_RETRIES")
                .ok()
                .and_then(|r| r.parse().ok())
                .unwrap_or(10),
            base_backoff: Duration::from_secs(base_secs),
            max_backoff: Duration::from_secs(base_secs * 60), // Max 60x base
            stale_timeout: Duration::from_secs(
                env::var("STALE_FILTER_TIMEOUT_SECS")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(600), // Default 10 minutes
            ),
        }
    }
}

impl ReconnectConfig {
    /// Calculate backoff duration for a given attempt (exponential backoff)
    pub fn backoff_for_attempt(&self, attempt: u32) -> Duration {
        let backoff = self.base_backoff.as_secs() * 2u64.saturating_pow(attempt);
        Duration::from_secs(backoff.min(self.max_backoff.as_secs()))
    }
}

/// Main application configuration
#[derive(Debug, Clone, Default)]
pub struct Config {
    pub server: ServerConfig,
    pub broadcast: BroadcastConfig,
    pub health: HealthCheckConfig,
    pub reconnect: ReconnectConfig,
}

impl Config {
    /// Load configuration from environment variables
    pub fn from_env() -> Self {
        Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.server.host, "0.0.0.0");
        assert_eq!(config.server.port, 8080);
        assert_eq!(config.broadcast.channel_capacity, 1000);
        assert_eq!(config.health.check_interval, Duration::from_secs(60));
        assert_eq!(config.reconnect.max_retries, 10);
    }

    #[test]
    fn test_reconnect_backoff() {
        let config = ReconnectConfig {
            max_retries: 5,
            base_backoff: Duration::from_secs(1),
            max_backoff: Duration::from_secs(30),
            stale_timeout: Duration::from_secs(600),
        };

        assert_eq!(config.backoff_for_attempt(0), Duration::from_secs(1));
        assert_eq!(config.backoff_for_attempt(1), Duration::from_secs(2));
        assert_eq!(config.backoff_for_attempt(2), Duration::from_secs(4));
        assert_eq!(config.backoff_for_attempt(3), Duration::from_secs(8));
        assert_eq!(config.backoff_for_attempt(4), Duration::from_secs(16));
        assert_eq!(config.backoff_for_attempt(5), Duration::from_secs(30)); // Capped at max
    }

    #[test]
    fn test_server_addr() {
        let config = ServerConfig {
            host: "127.0.0.1".to_string(),
            port: 3000,
        };
        assert_eq!(config.addr(), "127.0.0.1:3000");
    }
}
