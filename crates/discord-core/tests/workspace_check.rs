// M1.1 acceptance: workspace builds, 5 crates present, manifest matches plan.
#[test]
fn workspace_has_five_crates() {
    let root = env!("CARGO_MANIFEST_DIR");
    // core is at crates/discord-core; the workspace root is 2 levels up
    let ws = std::path::Path::new(root).parent().unwrap().parent().unwrap();
    let members = std::fs::read_dir(ws.join("crates")).unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    for expected in ["discord-core", "discord-auth", "discord-db", "discord-cli", "discord-mcp"] {
        assert!(members.contains(&expected.to_string()), "missing crate {}", expected);
    }
}

#[test]
fn discord_user_rs_is_core_dependency() {
    // Verify the plan's core crate is a dependency (declared in core's Cargo.toml)
    let manifest = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml")).unwrap();
    assert!(manifest.contains("discord-user-rs"), "discord-user-rs must be a dep of discord-core");
}
