use clap::Parser;
use ersa_lsp_core::lsp;

#[derive(Parser)]
#[command(name = "ersa_lsp")]
#[command(about = "GPC Language Server Protocol", long_about = None)]
struct Args {
    /// Start the language server using stdin/stdout
    #[arg(long)]
    stdio: bool,

    /// Show version information
    #[arg(long)]
    version: bool,

    /// Enable experimental features (comma-separated: imports, macros, code_lens, all)
    #[arg(long, value_delimiter = ',')]
    features: Option<Vec<String>>,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    if args.version {
        println!("{}", env!("CARGO_PKG_VERSION"));
    } else if args.stdio {
        let features = if let Some(feature_list) = args.features {
            lsp::Features::from_list(feature_list)
        } else {
            lsp::Features::none()
        };
        lsp::LSP::start(features).await;
    } else {
        eprintln!("Usage: ersa_lsp --stdio [--features <features>]");
        eprintln!("       ersa_lsp --version");
        eprintln!();
        eprintln!("Available features: imports, macros, code_lens, all");
    }
}
