use clap::{Args, Parser, Subcommand, ValueEnum};
use flate2::read::GzDecoder;
use hotprices_au_rs::cache::FsCache;
use hotprices_au_rs::stores::coles::product::load_from_legacy;
use hotprices_au_rs::stores::coles::{compress, fetch};
use std::io::BufReader;
use std::path::PathBuf;
use std::{fmt::Display, fs::File};
use time::{macros::format_description, Date, OffsetDateTime};

fn configure_logging(cli: &Cli) {
    let mut builder = env_logger::Builder::new();
    builder.filter_level(log::LevelFilter::Info);
    let log_level = match cli.debug {
        true => log::LevelFilter::Debug,
        false => log::LevelFilter::Info,
    };
    builder.filter_module("hotprices_au_rs", log_level);
    builder.init();
}

fn main() {
    let cli = Cli::parse();
    configure_logging(&cli);

    match cli.command {
        Commands::Sync(sync) => do_sync(sync, cli.output_dir),
        Commands::Analysis {
            day,
            store,
            compress,
            history,
        } => do_analysis(day, store, compress, history, cli.output_dir),
    }
}

fn do_sync(cmd: SyncCommand, output_dir: PathBuf) {
    let day = OffsetDateTime::now_utc().date().to_string();
    let cache_path = output_dir.join(cmd.store.to_string()).join(day);
    let cache: FsCache = FsCache::new(cache_path.clone());
    fetch(&cache, cmd.quick);
    compress(&cache_path);
}

fn do_analysis(day: Date, store: Store, compress: bool, history: bool, output_dir: PathBuf) {
    if history || compress {
        panic!("not implemented");
    }
    let file = output_dir
        .join(store.to_string())
        .join(format!("{day}.json.gz"));
    let file = File::open(file).unwrap();
    let file = GzDecoder::new(file);
    let file = BufReader::new(file);
    load_from_legacy(file).unwrap();
}

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[arg(long, default_value_t = false)]
    debug: bool,
    #[arg(long)]
    output_dir: PathBuf,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Sync(SyncCommand),
    Analysis {
        #[arg(long, value_parser = date_from_str, default_value_t = OffsetDateTime::now_utc().date())]
        day: Date,
        store: Store,
        #[arg(long, default_value_t = false)]
        compress: bool,
        #[arg(long, default_value_t = false)]
        history: bool,
    },
}

#[derive(Args)]
struct SyncCommand {
    #[arg(long, default_value_t = false)]
    quick: bool,
    #[arg(long, default_value_t = false)]
    print_save_path: bool,
    #[arg(long, default_value_t = false)]
    skip_existing: bool,
    store: Store,
}

fn date_from_str(s: &str) -> Result<Date, String> {
    let format = format_description!("[year]-[month]-[day]");
    match Date::parse(s, &format) {
        Ok(date) => Ok(date),
        Err(error) => Err(format!(
            "Error parsing date, use format year-month-day (e.g. 2023-12-31). The parser reported the following error: {}",
            error
        )),
    }
    // Err("Not implemented".to_string())
}

#[derive(ValueEnum, Clone)]
enum Store {
    Coles,
}

impl Display for Store {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Coles => write!(f, "coles"),
        }
    }
}

#[test]
fn verify_cli() {
    use clap::CommandFactory;
    Cli::command().debug_assert()
}
