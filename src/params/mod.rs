use clap::Parser;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
pub struct Args {
    /// Deduplicate icons
    #[arg(short, long)]
    pub dedup: bool,
    /// Deduplicate icons counter
    #[arg(short, long)]
    pub counter: bool,
    /// Verbose
    #[arg(short, long)]
    pub verbose: bool,
}
