use std::sync::Arc;

use clap::Parser;
use dioxus_tui::Config;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

mod bench;
mod tui;
mod whep;

use tui::dioxus_app;

/// Whep benchmarking tool
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Whep server url
    #[arg(env, long)]
    url: String,

    /// Whep server token
    #[arg(env, long)]
    token: String,

    /// Number of clients
    #[arg(env, long, default_value = "1")]
    count: usize,

    /// Interval between clients in miliseconds
    #[arg(env, long, default_value = "1000")]
    interval: u64,

    /// Life time of each client in miliseconds
    #[arg(env, long, default_value = "100000")]
    live: u64,

    /// Enable UI
    #[arg(env, long, default_value = "false")]
    ui: bool,
}

#[async_std::main]
async fn main() {
    let args: Args = Args::parse();
    let (event_tx, event_rx) = async_std::channel::unbounded::<bench::BenchEvent>();

    if args.ui {
        std::thread::spawn(|| {
            dioxus_tui::launch_cfg_with_props(
                dioxus_app,
                tui::AppProps {
                    rx: Arc::new(event_rx),
                },
                Config::default(),
            );
        });
    }

    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env())
        .init();

    let plan = bench::BenchPlan {
        count: args.count,
        interval: std::time::Duration::from_millis(args.interval),
        live: std::time::Duration::from_millis(args.live),
    };

    let mut runner = bench::BenchRunner::new(&args.url, &args.token, plan, event_tx);
    runner.bootstrap().await;
    loop {
        async_std::task::sleep(std::time::Duration::from_secs(1)).await;
    }
}
