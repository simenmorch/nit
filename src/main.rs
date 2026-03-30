mod app;
mod git;
mod model;
mod ui;

use anyhow::Result;
use clap::Parser;

#[derive(Parser)]
#[command(name = "nit", about = "Terminal code review tool")]
struct Cli {}

fn main() -> Result<()> {
    let _cli = Cli::parse();

    let repo = git::open_repo()?;
    let diff = git::get_uncommitted_diff(&repo)?;

    println!("{} file(s) changed:", diff.files.len());
    for file in &diff.files {
        println!("  {} (+{} -{})", file.path, file.added, file.removed);
    }

    Ok(())
}
