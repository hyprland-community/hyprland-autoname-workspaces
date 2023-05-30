use clap::Parser;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
pub struct Args {
    #[arg(short, long)]
    pub verbose: bool,
    #[arg(short, long)]
    pub debug: bool,
    #[arg(long)]
    pub dump: bool,
    #[arg(long)]
    pub migrate_config: bool,
    #[arg(short, long, default_value = None)]
    pub config: Option<String>,
}
