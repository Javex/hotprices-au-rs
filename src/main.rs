use anyhow::Context;
use clap::{Parser, Subcommand};
use hotprices_au_rs::analysis::{do_analysis, AnalysisType};
use hotprices_au_rs::stores::Store;
use hotprices_au_rs::sync::do_sync;
use log::error;
use std::path::PathBuf;
use std::result::Result as StdResult;
use time::{macros::format_description, Date, OffsetDateTime};

fn configure_logging(cli: &Cli) {
    let mut builder = env_logger::Builder::new();
    builder.filter_level(log::LevelFilter::Info);
    let log_level = if cli.debug {
        log::LevelFilter::Debug
    } else {
        log::LevelFilter::Info
    };
    builder.filter_module("hotprices_au_rs", log_level);
    builder.init();
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    configure_logging(&cli);

    let result = match cli.command {
        Commands::Sync {
            quick,
            print_save_path,
            skip_existing,
            store,
            cache_path,
        } => do_sync(
            store,
            quick,
            print_save_path,
            skip_existing,
            cli.output_dir,
            cache_path,
        ),
        Commands::Analysis {
            day,
            store,
            compress,
            history,
            data_dir,
        } => {
            let analysis_type = if history {
                AnalysisType::History
            } else {
                AnalysisType::Day(day)
            };
            do_analysis(analysis_type, store, compress, &cli.output_dir, &data_dir)
                .context("Failed to perform analysis")
        }
    };

    // Print error message if result contained an error
    if let Err(ref error) = result.as_ref() {
        error!("Unexpected error from program: {}", error);
    }
    result
}

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[arg(long, default_value_t = false)]
    debug: bool,
    #[arg(long, default_value = "output")]
    output_dir: PathBuf,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Sync {
        #[arg(long, default_value_t = false)]
        quick: bool,
        #[arg(long, default_value_t = false)]
        print_save_path: bool,
        #[arg(long, default_value_t = false)]
        skip_existing: bool,
        store: Store,
        #[arg(long, default_value = "cache")]
        cache_path: PathBuf,
    },
    Analysis {
        #[arg(long, value_parser = date_from_str, default_value_t = OffsetDateTime::now_utc().date())]
        day: Date,
        #[arg(long)]
        store: Option<Store>,
        #[arg(long, default_value_t = false)]
        compress: bool,
        #[arg(long, default_value_t = false)]
        history: bool,
        #[arg(long, default_value = "static/data")]
        data_dir: PathBuf,
    },
}

fn date_from_str(s: &str) -> StdResult<Date, String> {
    let format = format_description!("[year]-[month]-[day]");
    match Date::parse(s, &format) {
        Ok(date) => Ok(date),
        Err(error) => Err(format!(
            "Error parsing date, use format year-month-day (e.g. 2023-12-31). The parser reported the following error: {error}",
        )),
    }
}

#[test]
fn verify_cli() {
    use clap::CommandFactory;
    Cli::command().debug_assert();
}
