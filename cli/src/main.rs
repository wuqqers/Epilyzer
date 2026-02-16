use clap::{Parser, Subcommand};
use anyhow::{Context, Result};
use tokio::net::UnixStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use core::ipc::{IpcCommand, IpcResponse};
use std::process::exit;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the daemon (not implemented in CLI, use systemd)
    Start,
    /// Stop the daemon (not implemented in CLI)
    Stop,
    /// Set absolute brightness
    Set {
        #[arg(value_parser = clap::value_parser!(f64))]
        value: f64,
    },
    /// Emergency freeze
    Freeze,
    /// Get current status
    Info,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    
    let command = match cli.command {
        Commands::Set { value } => IpcCommand::SetBrightness(value),
        Commands::Freeze => IpcCommand::Freeze(300),
        Commands::Info => IpcCommand::GetInfo,
        _ => {
            println!("Start/Stop should be managed via systemctl.");
            exit(0);
        }
    };

    send_ipc(command).await?;
    Ok(())
}

async fn send_ipc(cmd: IpcCommand) -> Result<()> {
    let socket_path = "/tmp/auto_brightness.sock";
    let mut stream = UnixStream::connect(socket_path).await.context("Could not connect to daemon. Is it running?")?;

    let bytes = serde_json::to_vec(&cmd)?;
    stream.write_all(&bytes).await?;
    
    // Read response
    let mut buf = [0; 1024];
    let n = stream.read(&mut buf).await?;
    if n > 0 {
        let resp: IpcResponse = serde_json::from_slice(&buf[..n])?;
        match resp {
            IpcResponse::Ok => println!("OK"),
            IpcResponse::Error(e) => eprintln!("Error: {}", e),
            IpcResponse::Status { brightness, location, wake_time, transition_duration_ms, flashbang_protection } => {
                println!("--- AutoBrightness Status ---");
                println!("Brightness:       {:.1}%", brightness);
                println!("Location:         {}", location);
                println!("Wake Time:        {}", wake_time);
                println!("Transition Time:  {}ms", transition_duration_ms);
                println!("Flashbang Prot.:  {}", if flashbang_protection { "ON" } else { "OFF" });
            }
        }
    } else {
        eprintln!("No response from daemon");
    }

    Ok(())
}
