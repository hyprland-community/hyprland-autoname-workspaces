use clap::Parser;

#[derive(Parser)]
pub struct Args {
    /// Deduplicate icons
    #[arg(short, long)]
    pub dedup: bool,
}
