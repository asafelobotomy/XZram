//! `data/bash-completion/xzram` is generated from the clap definition, so it can't drift
//! from the CLI. Regenerate with `make completions` (sets XZRAM_UPDATE_COMPLETIONS=1).

use std::path::PathBuf;

use clap::CommandFactory;

use crate::args::Cli;

fn generated_bash() -> String {
    let mut out = Vec::new();
    clap_complete::generate(
        clap_complete::Shell::Bash,
        &mut Cli::command(),
        "xzram",
        &mut out,
    );
    let script = String::from_utf8(out).expect("clap_complete writes UTF-8");
    format!(
        "# bash completion for xzram\n\
         # Generated from the CLI definition by clap_complete; do not edit.\n\
         # Regenerate with: make completions\n\n{script}"
    )
}

#[test]
fn bash_completion_matches_the_cli() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data/bash-completion/xzram");
    let want = generated_bash();
    if std::env::var_os("XZRAM_UPDATE_COMPLETIONS").is_some() {
        std::fs::write(&path, &want).unwrap();
        return;
    }
    let have = std::fs::read_to_string(&path).unwrap();
    assert!(
        have == want,
        "data/bash-completion/xzram is out of date with the CLI; run `make completions`"
    );
}
