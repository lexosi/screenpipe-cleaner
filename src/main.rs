/// screenpipe-manager: CLI tool to manage screenpipe recordings and configuration.
///
/// Subcommands:
///   cleanup  - Remove old DB records and physical files based on retention policy
///   status   - Display current config, disk usage, and DB record counts
mod cleanup;
mod config;
mod filter;
mod storage;

use clap::{Parser, Subcommand};
use std::process;

#[derive(Parser)]
#[command(
    name = "screenpipe-manager",
    about = "Manage screenpipe recordings, retention, and configuration",
    version,
    author
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Delete old DB records and associated files based on retention policy
    Cleanup {
        /// Preview what would be deleted without making any changes
        #[arg(long)]
        dry_run: bool,

        /// Override the retention_days value from config
        #[arg(long)]
        days: Option<u32>,
    },

    /// Show config values, disk usage, and DB record counts
    Status,
}

fn main() {
    let cli = Cli::parse();

    // Load config from the directory where the binary lives, falling back to CWD.
    let cfg = match config::load_config() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error loading config: {e}");
            process::exit(1);
        }
    };

    let result = match cli.command {
        Commands::Cleanup { dry_run, days } => cleanup::run(&cfg, dry_run, days),
        Commands::Status => {
            status::run(&cfg);
            Ok(())
        }
    };

    if let Err(e) = result {
        eprintln!("Error: {e}");
        process::exit(1);
    }
}

/// Inline status module — thin orchestration layer that calls storage and config helpers.
mod status {
    use crate::{config::Config, storage};

    pub fn run(cfg: &Config) {
        print_config(cfg);
        print_disk_usage(cfg);
        if let Err(e) = print_db_counts(cfg) {
            eprintln!("Could not read DB counts: {e}");
        }
    }

    fn print_config(cfg: &Config) {
        println!("=== Configuration ===");
        println!("  retention_days        : {}", cfg.retention_days);
        println!("  max_storage_gb        : {:.1}", cfg.max_storage_gb);
        println!(
            "  data_dir              : {}",
            cfg.resolved_data_dir().display()
        );
        println!("  record_audio          : {}", cfg.record_audio);
        println!("  record_screen         : {}", cfg.record_screen);
        println!("  record_transcription  : {}", cfg.record_transcription);

        if cfg.blacklist.is_empty() {
            println!("  blacklist             : (none)");
        } else {
            println!("  blacklist             : {}", cfg.blacklist.join(", "));
        }

        if cfg.whitelist.is_empty() {
            println!("  whitelist             : (none — record all apps)");
        } else {
            println!("  whitelist             : {}", cfg.whitelist.join(", "));
        }
    }

    fn print_disk_usage(cfg: &Config) {
        println!("\n=== Disk Usage ===");
        let data_dir = cfg.resolved_data_dir();
        match storage::directory_size_bytes(&data_dir) {
            Ok(bytes) => {
                let mb = bytes as f64 / 1_048_576.0;
                let gb = bytes as f64 / 1_073_741_824.0;
                if gb >= 1.0 {
                    println!("  {:.2} GB  ({})", gb, data_dir.display());
                } else {
                    println!("  {:.1} MB  ({})", mb, data_dir.display());
                }
            }
            Err(e) => println!("  Could not compute disk usage: {e}"),
        }
    }

    fn print_db_counts(cfg: &Config) -> Result<(), Box<dyn std::error::Error>> {
        let db_path = cfg.resolved_data_dir().join("db.sqlite");
        println!("\n=== Database Record Counts ===");
        println!("  DB path: {}", db_path.display());

        let conn = rusqlite::Connection::open(&db_path)?;
        for table in &["frames", "audio_chunks", "audio_transcriptions", "ocr_text", "video_chunks"] {
            let count: i64 = conn.query_row(
                &format!("SELECT COUNT(*) FROM {table}"),
                [],
                |row| row.get(0),
            ).unwrap_or(-1);

            if count < 0 {
                println!("  {table:<24}: (table not found)");
            } else {
                println!("  {table:<24}: {count} records");
            }
        }
        Ok(())
    }
}
