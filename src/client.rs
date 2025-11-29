use anyhow::{Context, Result};
use std::io::Read;
use std::{env, fs, io};

use crate::ipc::{IpcClient, IpcMessage};

pub struct Client;

impl Client {
    pub async fn send_image(filename: &str) -> Result<()> {
        let filename = if filename == "-" {
            let temp_dir = env::temp_dir();
            let temp_path = temp_dir.join(format!("satty-{}.png", std::process::id()));

            let mut buf = Vec::<u8>::new();
            io::stdin().lock().read_to_end(&mut buf)
                .context("Failed to read image from stdin")?;

            fs::write(&temp_path, buf)
                .context("Failed to write temporary file")?;

            temp_path.to_string_lossy().to_string()
        } else {
            fs::canonicalize(filename)
                .context(format!("Failed to resolve image file path: {}", filename))?
                .to_string_lossy()
                .to_string()
        };

        let message = IpcMessage::LoadImage { filename };

        match IpcClient::send_message(&message).await {
            Ok(_) => {
                eprintln!("Image sent to daemon successfully");
                Ok(())
            }
            Err(e) => {
                eprintln!("Failed to send image to daemon: {}", e);
                Err(e)
            }
        }
    }

    pub async fn ping() -> Result<()> {
        let message = IpcMessage::Ping;
        IpcClient::send_message(&message).await?;
        eprintln!("Daemon is running");
        Ok(())
    }

    pub async fn shutdown() -> Result<()> {
        let message = IpcMessage::Shutdown;
        IpcClient::send_message(&message).await?;
        eprintln!("Shutdown signal sent to daemon");
        Ok(())
    }
}
