//! Disassemble `.ssb` script files (the program's bytecode; see `pd_formats::ssb`).
//!
//!   cargo run -p pd_formats --example ssbdis -- <file.ssb | dir> [more...]
//!
//! Prints the listing to stdout. The output is derived from game data: do not commit it.
use pd_formats::ssb;
use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    let args: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    if args.is_empty() {
        anyhow::bail!("usage: ssbdis <file.ssb | dir> ...");
    }
    for a in args {
        let mut files = Vec::new();
        if a.is_dir() {
            for e in std::fs::read_dir(&a)? {
                let p = e?.path();
                if p.extension().is_some_and(|x| x == "ssb") {
                    files.push(p);
                }
            }
            files.sort();
        } else {
            files.push(a);
        }
        for f in files {
            println!("; ===== {} =====", f.display());
            match ssb::load(&f) {
                Ok(s) => print!("{}", s.disassembly_text()),
                Err(e) => println!("; parse failed: {e:#}"),
            }
        }
    }
    Ok(())
}
