//! Neovim RPC connection for cursor placement and filetype tracking
//!
//! This module connects to a running neovim instance via its Unix socket.

use std::path::Path;
use std::time::Duration;

use nvim_rs::compat::tokio::Compat;
use nvim_rs::create::tokio::new_path;
use nvim_rs::rpc::handler::Dummy;
use nvim_rs::{Neovim, Value};
use tokio::io::WriteHalf;
use tokio::net::UnixStream;

/// Type alias for the neovim connection writer
type NvimWriter = Compat<WriteHalf<UnixStream>>;

/// Active RPC session with neovim
pub struct NvimRpcSession {
    neovim: Neovim<NvimWriter>,
}

impl NvimRpcSession {
    /// Set cursor position in nvim (0-based line and column)
    pub async fn set_cursor(&self, line: usize, column: usize) -> Result<(), String> {
        // nvim uses 1-based line numbers and 0-based column
        let nvim_line = (line + 1) as i64;
        let nvim_col = column as i64;

        self.neovim
            .get_current_win()
            .await
            .map_err(|e| format!("Failed to get current window: {}", e))?
            .set_cursor((nvim_line, nvim_col))
            .await
            .map_err(|e| format!("Failed to set cursor: {}", e))?;

        Ok(())
    }

    /// Get cursor position from nvim (returns 0-based line and column)
    pub async fn get_cursor(&self) -> Result<(usize, usize), String> {
        let (line, col) = self.neovim
            .get_current_win()
            .await
            .map_err(|e| format!("Failed to get current window: {}", e))?
            .get_cursor()
            .await
            .map_err(|e| format!("Failed to get cursor: {}", e))?;

        // Convert from nvim's 1-based line to 0-based
        Ok(((line - 1) as usize, col as usize))
    }

    /// Get the current filetype from nvim
    pub async fn get_filetype(&self) -> Result<String, String> {
        let opts = vec![(Value::from("scope"), Value::from("local"))];
        let value = self
            .neovim
            .get_option_value("filetype", opts)
            .await
            .map_err(|e| format!("Failed to get filetype: {}", e))?;

        value
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| "Filetype is not a string".to_string())
    }
}

/// Connect to a running neovim instance via Unix socket
///
/// Retries connection with exponential backoff since nvim takes time to start.
/// Returns an error if the connection fails after all retries.
pub async fn connect_to_nvim(socket_path: &Path) -> Result<NvimRpcSession, String> {
    // Retry with exponential backoff
    let mut delay = Duration::from_millis(100);
    let max_delay = Duration::from_secs(5);
    let mut total_waited = Duration::ZERO;

    let (neovim, io_handler) = loop {
        match new_path(socket_path, Dummy::new()).await {
            Ok(result) => break result,
            Err(e) => {
                if total_waited >= max_delay {
                    return Err(format!(
                        "Failed to connect to nvim socket after {:?}: {}",
                        total_waited, e
                    ));
                }
                log::debug!(
                    "Waiting for nvim socket ({}ms elapsed): {}",
                    total_waited.as_millis(),
                    e
                );
                tokio::time::sleep(delay).await;
                total_waited += delay;
                delay = (delay * 2).min(Duration::from_secs(1));
            }
        }
    };

    // Spawn the IO handler in background
    tokio::spawn(async move {
        match io_handler.await {
            Ok(Ok(())) => log::debug!("Neovim IO handler finished normally"),
            Ok(Err(e)) => {
                // Log at debug level since disconnect is expected when nvim exits
                log::debug!("Neovim IO handler finished with error: {:?}", e);
            }
            Err(e) => {
                log::debug!("Neovim IO handler task failed: {:?}", e);
            }
        }
    });

    log::info!("Connected to nvim at {:?}", socket_path);

    Ok(NvimRpcSession { neovim })
}

/// Check if the socket file exists
pub fn socket_exists(socket_path: &Path) -> bool {
    socket_path.exists()
}
