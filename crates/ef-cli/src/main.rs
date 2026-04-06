use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "ef", about = "EdgeFabric CLI", version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// List available devices
    Devices {
        /// Filter by OS
        #[arg(long)]
        os: Option<String>,
        /// Filter by status
        #[arg(long)]
        status: Option<String>,
    },
    /// Create an interactive device session
    Session {
        /// Device filter (OS)
        #[arg(long)]
        os: Option<String>,
        /// Timeout in minutes
        #[arg(long, default_value = "30")]
        timeout: u32,
    },
    /// Submit a test job
    Test {
        /// Path to application binary
        #[arg(long)]
        app: String,
        /// Path to test suite
        #[arg(long)]
        suite: String,
        /// Number of parallel devices
        #[arg(long, default_value = "1")]
        parallelism: u32,
    },
    /// Check system health
    Health,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Devices { os, status } => {
            println!("Listing devices (os={os:?}, status={status:?})...");
            // TODO: call API
        }
        Commands::Session { os, timeout } => {
            println!("Creating session (os={os:?}, timeout={timeout}m)...");
            // TODO: call API
        }
        Commands::Test {
            app,
            suite,
            parallelism,
        } => {
            println!("Submitting test job (app={app}, suite={suite}, parallelism={parallelism})...");
            // TODO: call API
        }
        Commands::Health => {
            println!("Checking health...");
            // TODO: call API /health
        }
    }

    Ok(())
}
