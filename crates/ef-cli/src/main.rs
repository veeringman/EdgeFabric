use anyhow::Result;
use clap::{Parser, Subcommand};

mod client;
mod commands;
mod config;
mod output;

use config::load_config;
use output::OutputFormat;

#[derive(Parser)]
#[command(name = "ef", about = "EdgeFabric CLI — manage devices, sessions, and test jobs", version)]
struct Cli {
    /// Output format: table, json
    #[arg(long, short = 'o', global = true)]
    format: Option<String>,

    /// API server URL (overrides config)
    #[arg(long, global = true)]
    server: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Authenticate with the EdgeFabric API
    Login {
        /// API key to exchange for a JWT token
        #[arg(long)]
        api_key: String,
    },
    /// Remove stored credentials
    Logout,

    /// Manage devices
    #[command(subcommand)]
    Devices(DevicesCmd),

    /// Manage sessions
    #[command(subcommand)]
    Session(SessionCmd),

    /// Manage test jobs
    #[command(subcommand)]
    Test(TestCmd),

    /// Check API server health
    Health,
}

#[derive(Subcommand)]
enum DevicesCmd {
    /// List all devices
    List {
        #[arg(long)]
        os: Option<String>,
        #[arg(long)]
        status: Option<String>,
        #[arg(long)]
        region: Option<String>,
    },
    /// Show device details
    Get {
        /// Device UUID
        id: String,
    },
    /// Check device health
    Health {
        /// Device UUID
        id: String,
    },
}

#[derive(Subcommand)]
enum SessionCmd {
    /// List sessions for the current tenant
    List,
    /// Show session details
    Get {
        /// Session UUID
        id: String,
    },
    /// Create a new session on a device
    Create {
        /// Device UUID to attach
        #[arg(long)]
        device_id: String,
        /// Timeout in minutes
        #[arg(long, default_value = "30")]
        timeout: i64,
    },
    /// End a session
    End {
        /// Session UUID
        id: String,
    },
}

#[derive(Subcommand)]
enum TestCmd {
    /// List test jobs
    List,
    /// Show job details (with test cases)
    Get {
        /// Job UUID
        id: String,
    },
    /// Submit a new test job
    Submit {
        /// Test suite name
        #[arg(long)]
        suite: String,
        /// Comma-separated test names
        #[arg(long, value_delimiter = ',')]
        tests: Vec<String>,
        /// Number of parallel devices
        #[arg(long, default_value = "1")]
        parallelism: i32,
        /// Max retries per test
        #[arg(long, default_value = "2")]
        retries: i32,
        /// Webhook URL for completion notification
        #[arg(long)]
        webhook: Option<String>,
    },
    /// Show job progress
    Progress {
        /// Job UUID
        id: String,
    },
    /// Watch a job until completion
    Watch {
        /// Job UUID
        id: String,
    },
    /// Cancel a running job
    Cancel {
        /// Job UUID
        id: String,
    },
    /// List job artifacts
    Artifacts {
        /// Job UUID
        id: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let cfg = load_config()?;

    let base_url = cli.server.as_deref().unwrap_or(&cfg.server_url);
    let format = OutputFormat::from_str(
        cli.format.as_deref().unwrap_or(&cfg.output_format),
    );

    match cli.command {
        Commands::Login { api_key } => {
            commands::auth::login(base_url, &api_key, format).await?;
        }
        Commands::Logout => {
            commands::auth::logout().await?;
        }
        Commands::Health => {
            commands::health::check(base_url, format).await?;
        }
        Commands::Devices(sub) => {
            let api = client::ApiClient::new(base_url)?;
            match sub {
                DevicesCmd::List { os, status, region } => {
                    commands::devices::list(
                        &api,
                        os.as_deref(),
                        status.as_deref(),
                        region.as_deref(),
                        format,
                    )
                    .await?;
                }
                DevicesCmd::Get { id } => {
                    commands::devices::get(&api, &id, format).await?;
                }
                DevicesCmd::Health { id } => {
                    commands::devices::health(&api, &id, format).await?;
                }
            }
        }
        Commands::Session(sub) => {
            let api = client::ApiClient::new(base_url)?;
            match sub {
                SessionCmd::List => {
                    commands::sessions::list(&api, format).await?;
                }
                SessionCmd::Get { id } => {
                    commands::sessions::get(&api, &id, format).await?;
                }
                SessionCmd::Create {
                    device_id,
                    timeout,
                } => {
                    commands::sessions::create(&api, &device_id, timeout, format).await?;
                }
                SessionCmd::End { id } => {
                    commands::sessions::end(&api, &id, format).await?;
                }
            }
        }
        Commands::Test(sub) => {
            let api = client::ApiClient::new(base_url)?;
            match sub {
                TestCmd::List => {
                    commands::test::list(&api, format).await?;
                }
                TestCmd::Get { id } => {
                    commands::test::get(&api, &id, format).await?;
                }
                TestCmd::Submit {
                    suite,
                    tests,
                    parallelism,
                    retries,
                    webhook,
                } => {
                    commands::test::submit(
                        &api,
                        &suite,
                        tests,
                        parallelism,
                        retries,
                        webhook.as_deref(),
                        format,
                    )
                    .await?;
                }
                TestCmd::Progress { id } => {
                    commands::test::progress(&api, &id, format).await?;
                }
                TestCmd::Watch { id } => {
                    commands::test::watch(&api, &id).await?;
                }
                TestCmd::Cancel { id } => {
                    commands::test::cancel(&api, &id, format).await?;
                }
                TestCmd::Artifacts { id } => {
                    commands::test::artifacts(&api, &id, format).await?;
                }
            }
        }
    }

    Ok(())
}
