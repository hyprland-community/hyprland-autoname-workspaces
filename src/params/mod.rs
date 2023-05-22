use clap::Parser;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
pub struct Args {
    /// Verbose
    #[arg(short, long)]
    pub verbose: bool,
    #[arg(short, long)]
    pub dump: bool,
    /// Config file
    #[arg(short, long, default_value = None)]
    pub config: Option<String>,
}
