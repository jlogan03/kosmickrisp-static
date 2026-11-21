use std::{env, path::PathBuf};

fn main() -> anyhow::Result<()> {
    let mut args = env::args_os().skip(1);
    let out = args.next().map(PathBuf::from);
    anyhow::ensure!(
        out.is_some() && args.next().is_none(),
        "usage: cargo run -p kosmickrisp-build -- <output-directory>"
    );
    let out = out.unwrap();
    let artifact = kosmickrisp_build::prepare(&out, true)?;
    println!("{}", kosmickrisp_build::package(&artifact, &out)?.display());
    Ok(())
}
