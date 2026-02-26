use clap::Parser;
use discovery_server::{run_server, DEFAULT_PORT};
use simplelog::{ColorChoice, Config, LevelFilter, TermLogger, TerminalMode};

#[derive(Parser, Debug)]
#[command(name = "discovery_server")]
#[command(about = "Central discovery server for bspterm LAN user discovery")]
struct Args {
    /// Port to listen on
    #[arg(short, long, default_value_t = DEFAULT_PORT)]
    port: u16,

    /// Address to bind to
    #[arg(short, long, default_value = "0.0.0.0")]
    bind: String,

    /// Log level (error, warn, info, debug, trace)
    #[arg(short, long, default_value = "info")]
    log_level: String,
}

fn parse_log_level(level: &str) -> LevelFilter {
    match level.to_lowercase().as_str() {
        "error" => LevelFilter::Error,
        "warn" => LevelFilter::Warn,
        "info" => LevelFilter::Info,
        "debug" => LevelFilter::Debug,
        "trace" => LevelFilter::Trace,
        _ => LevelFilter::Info,
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    TermLogger::init(
        parse_log_level(&args.log_level),
        Config::default(),
        TerminalMode::Mixed,
        ColorChoice::Auto,
    )?;

    log::info!(
        "Starting discovery server on {}:{}",
        args.bind,
        args.port
    );

    run_server(&args.bind, args.port).await
}
