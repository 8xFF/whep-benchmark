use clap::Parser;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

mod whep;

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
}

#[async_std::main]
async fn main() {
    if std::env::var_os("RUST_LOG").is_none() {
        std::env::set_var("RUST_LOG", "atm0s_media_server=info");
    }
    let args: Args = Args::parse();
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env())
        .init();

    let mut client =
        whep::WhepClient::new(&args.url, &args.token).expect("should create whep client");
    client.prepare().await.expect("should connect");
    loop {
        match client.recv().await {
            Ok(event) => match event {
                whep::WhepEvent::Connected => {
                    log::info!("[WhepClient] connected");
                }
                whep::WhepEvent::Disconnected => {
                    log::info!("[WhepClient] disconnected");
                    break;
                }
                whep::WhepEvent::Stats(stats) => {
                    log::info!("[WhepClient] stats: {:?}", stats);
                }
                whep::WhepEvent::Continue => {}
            },
            Err(err) => {
                log::error!("[WhepClient] error: {:?}", err);
                break;
            }
        }
    }
}
