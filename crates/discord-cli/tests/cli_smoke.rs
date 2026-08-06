// M1.4 integration: CLI exits and envelopes behave without a token.
use std::process::Command;

/// Spawn the binary with no token source (empty env + no .env + empty flag).
fn no_token_cmd() -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_discord"));
    c.env("DISCORD_TOKEN", "")
        .env(
            "DATA_DIR",
            std::env::temp_dir().join("discord-test-no-token"),
        )
        .env("NO_COLOR", "1");
    c
}

#[test]
fn status_without_token_exits_1() {
    let out = no_token_cmd().arg("status").output().unwrap();
    assert_eq!(
        out.status.code(),
        Some(1),
        "status must exit 1 without token"
    );
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
    let out = Command::new(env!("CARGO_BIN_EXE_discord"))
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Usage:"));
}

#[test]
fn dc_guilds_without_token_exits_1() {
    let out = no_token_cmd().args(["guilds"]).output().unwrap();
    assert_eq!(
        out.status.code(),
        Some(1),
        "guilds must exit 1 without token"
    );
}

#[test]
fn dc_help_lists_subcommands() {
    let out = Command::new(env!("CARGO_BIN_EXE_discord"))
        .args(["--help"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("guilds"));
    assert!(stdout.contains("channels"));
}

#[test]
fn send_without_confirm_exits_2() {
    let out = Command::new(env!("CARGO_BIN_EXE_discord"))
        .args(["send", "123456", "--text", "hi"])
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(2),
        "send must exit 2 without --confirm"
    );
}

#[test]
fn send_dry_run_exits_0_with_preview() {
    let out = Command::new(env!("CARGO_BIN_EXE_discord"))
        .args(["send", "123456", "--text", "hi", "--dry-run"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("\"action\":\"send_message\""));
}

#[test]
fn serve_command_exists() {
    let out = Command::new(env!("CARGO_BIN_EXE_discord"))
        .args(["serve", "--help"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("MCP server"), "stdout: {stdout}");
}

#[test]
fn watch_command_exists() {
    let out = Command::new(env!("CARGO_BIN_EXE_discord"))
        .args(["watch", "--help"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("JSONL stream"));
}

#[test]
fn dm_group_create_requires_confirm() {
    let out = Command::new(env!("CARGO_BIN_EXE_discord"))
        .args(["dm-group", "create", "123,456"])
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(2),
        "dm-group create must exit 2 without --confirm"
    );
}
