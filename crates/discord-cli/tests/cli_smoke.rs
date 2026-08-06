// M1.4 integration: CLI exits and envelopes behave without a token.
use std::process::Command;

#[test]
fn status_without_token_exits_1() {
    let out = Command::new(env!("CARGO_BIN_EXE_discord"))
        .arg("status")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1), "status must exit 1 without token");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("DISCORD_TOKEN"), "stderr: {stderr}");
}

#[test]
fn help_shows_commands() {
    let out = Command::new(env!("CARGO_BIN_EXE_discord"))
        .arg("--help")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("status"));
    assert!(stdout.contains("whoami"));
    assert!(stdout.contains("WARNING"));
}

#[test]
fn no_subcommand_shows_help_exit_0() {
    let out = Command::new(env!("CARGO_BIN_EXE_discord")).output().unwrap();
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Usage:"));
}
