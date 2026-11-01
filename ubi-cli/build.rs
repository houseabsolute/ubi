use clap::ValueEnum;
use clap_complete_command::Shell;
use cmd::cmd;
use std::io;

#[path = "src/cmd.rs"]
mod cmd;

fn main() -> io::Result<()> {
    build_rs::output::rerun_if_changed("src/cmd.rs");
    let out_dir = build_rs::input::out_dir();
    for variant in Shell::value_variants() {
        variant.generate_to(&mut cmd(), &out_dir)?;
    }
    Ok(())
}
