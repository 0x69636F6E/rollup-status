use crate::config::ReconnectConfig;
use std::future::Future;
use tokio_util::sync::CancellationToken;

/// Result of a reconnection attempt
#[derive(Debug)]
pub enum ReconnectResult<T> {
    /// Successfully connected
    Connected(T),
    /// Max retries exceeded
    MaxRetriesExceeded,
    /// Cancelled via token
    Cancelled,
}

/// Attempt to establish a connection with exponential backoff.
///
/// This function will retry the connection function up to `config.max_retries` times,
/// with exponential backoff between attempts. It respects the cancellation token
/// and will return `Cancelled` if the token is triggered.
///
/// # Arguments
/// * `rollup` - Name of the rollup (for logging)
/// * `stream_name` - Name of the stream being connected (for logging)
/// * `config` - Reconnection configuration
/// * `cancel_token` - Cancellation token to stop reconnection attempts
/// * `connect_fn` - Async function that attempts to establish a connection
pub async fn connect_with_retry<T, E, F, Fut>(
    rollup: &str,
    stream_name: &str,
    config: &ReconnectConfig,
    cancel_token: &CancellationToken,
    connect_fn: F,
) -> ReconnectResult<T>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<T, E>>,
    E: std::fmt::Debug,
{
    let mut attempt = 0;

    loop {
        // Check cancellation before attempting
        if cancel_token.is_cancelled() {
            tracing::info!(
                rollup = rollup,
                stream = stream_name,
                "Reconnection cancelled"
            );
            return ReconnectResult::Cancelled;
        }

        match connect_fn().await {
            Ok(connection) => {
                if attempt > 0 {
                    tracing::info!(
                        rollup = rollup,
                        stream = stream_name,
                        attempts = attempt + 1,
                        "Reconnected successfully"
                    );
                }
                return ReconnectResult::Connected(connection);
            }
            Err(e) => {
                attempt += 1;

                if attempt >= config.max_retries {
                    tracing::error!(
                        rollup = rollup,
                        stream = stream_name,
                        attempts = attempt,
                        error = ?e,
                        "Max reconnection attempts exceeded"
                    );
                    return ReconnectResult::MaxRetriesExceeded;
                }

                let backoff = config.backoff_for_attempt(attempt);
                tracing::warn!(
                    rollup = rollup,
                    stream = stream_name,
                    attempt = attempt,
                    max_retries = config.max_retries,
                    backoff_secs = backoff.as_secs(),
                    error = ?e,
                    "Connection failed, retrying"
                );

                // Wait with cancellation support
                tokio::select! {
                    _ = tokio::time::sleep(backoff) => {}
                    _ = cancel_token.cancelled() => {
                        tracing::info!(
                            rollup = rollup,
                            stream = stream_name,
                            "Reconnection cancelled during backoff"
                        );
                        return ReconnectResult::Cancelled;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    #[tokio::test]
    async fn test_connect_with_retry_success_first_try() {
        let config = ReconnectConfig {
            max_retries: 3,
            base_backoff: std::time::Duration::from_millis(10),
            max_backoff: std::time::Duration::from_millis(100),
            stale_timeout: std::time::Duration::from_secs(600),
        };
        let cancel_token = CancellationToken::new();

        let result = connect_with_retry("test", "stream", &config, &cancel_token, || async {
            Ok::<_, &str>("connected")
        })
        .await;

        match result {
            ReconnectResult::Connected(val) => assert_eq!(val, "connected"),
            _ => panic!("Expected Connected result"),
        }
    }

    #[tokio::test]
    async fn test_connect_with_retry_success_after_failures() {
        let config = ReconnectConfig {
            max_retries: 5,
            base_backoff: std::time::Duration::from_millis(1),
            max_backoff: std::time::Duration::from_millis(10),
            stale_timeout: std::time::Duration::from_secs(600),
        };
        let cancel_token = CancellationToken::new();
        let attempts = Arc::new(AtomicU32::new(0));
        let attempts_clone = attempts.clone();

        let result = connect_with_retry("test", "stream", &config, &cancel_token, || {
            let attempts = attempts_clone.clone();
            async move {
                let attempt = attempts.fetch_add(1, Ordering::SeqCst);
                if attempt < 2 {
                    Err("not yet")
                } else {
                    Ok("connected")
                }
            }
        })
        .await;

        match result {
            ReconnectResult::Connected(val) => assert_eq!(val, "connected"),
            _ => panic!("Expected Connected result"),
        }
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_connect_with_retry_max_retries() {
        let config = ReconnectConfig {
            max_retries: 3,
            base_backoff: std::time::Duration::from_millis(1),
            max_backoff: std::time::Duration::from_millis(10),
            stale_timeout: std::time::Duration::from_secs(600),
        };
        let cancel_token = CancellationToken::new();

        let result = connect_with_retry("test", "stream", &config, &cancel_token, || async {
            Err::<(), _>("always fails")
        })
        .await;

        match result {
            ReconnectResult::MaxRetriesExceeded => {}
            _ => panic!("Expected MaxRetriesExceeded result"),
        }
    }

    #[tokio::test]
    async fn test_connect_with_retry_cancelled() {
        let config = ReconnectConfig {
            max_retries: 10,
            base_backoff: std::time::Duration::from_secs(100),
            max_backoff: std::time::Duration::from_secs(100),
            stale_timeout: std::time::Duration::from_secs(600),
        };
        let cancel_token = CancellationToken::new();
        cancel_token.cancel();

        let result = connect_with_retry("test", "stream", &config, &cancel_token, || async {
            Err::<(), _>("fails")
        })
        .await;

        match result {
            ReconnectResult::Cancelled => {}
            _ => panic!("Expected Cancelled result"),
        }
    }
}
