use ersa_lsp_core::lsp;

#[tokio::main]
async fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    if args.len() > 1 && args[1] == "--stdio" {
        lsp::LSP::start().await;
    } else if args.len() > 1 && args[1] == "--version" {
        println!("{}", env!("CARGO_PKG_VERSION"));
    } else {
        eprintln!("Usage: ersa_lsp [--stdio | --version]");
    }
}
