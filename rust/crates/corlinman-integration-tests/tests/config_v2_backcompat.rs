//! Test 3 — `config_v2_backcompat_plus_new_sections`.
//!
//! Two scenarios:
//!   1. A near-empty legacy TOML (no B1-BE4 sections at all) still parses,
//!      picks up defaults for the ten new sections, and validates.
//!   2. A TOML that populates every new section round-trips field values and
//!      also validates.
//!
//! `validate()` intentionally treats "no provider enabled" as a warning — not
//! an error — so a fresh-out-of-the-box config is valid. We rely on that here.

use corlinman_core::config::Config;

const LEGACY_ONLY: &str = r#"
# Only sections that existed before B1-BE4. Everything else must default.
[server]
port = 6005
bind = "0.0.0.0"
session_max_messages = 100

[logging]
level = "info"
format = "json"
file_rolling = false
"#;

const FULL_V2: &str = r#"
[server]
port = 6006
bind = "127.0.0.1"
session_max_messages = 250

[logging]
level = "debug"
format = "text"
file_rolling = true

[hooks]
capacity = 2048
enabled = true

[skills]
dir = "skills"
autoload = false

[variables]
tar_dir = "TVStxt/tar"
var_dir = "TVStxt/var"
sar_dir = "TVStxt/sar"
fixed_dir = "TVStxt/fixed"
hot_reload = false

[agents]
dir = "agents"
single_agent_gate = false

[tools.block]
enabled = true
fallback_to_function_call = false

[telegram.webhook]
public_url = "https://example.test/tg"
secret_token = "s3cr3t"
drop_updates_on_reconnect = true

[vector.tags]
hierarchy_enabled = true
max_depth = 8

[wstool]
bind = "127.0.0.1:18790"
auth_token = ""
heartbeat_secs = 30

[canvas]
host_endpoint_enabled = true
session_ttl_secs = 900

[nodebridge]
listen = "127.0.0.1:18788"
accept_unsigned = false
"#;

fn parse(toml_src: &str) -> Config {
    toml::from_str::<Config>(toml_src)
        .unwrap_or_else(|e| panic!("config TOML failed to parse: {e}"))
}

#[test]
fn legacy_only_config_gets_defaults_for_new_sections() {
    let cfg = parse(LEGACY_ONLY);

    // Defaults (see HooksConfig::default / SkillsConfig::default / etc.)
    assert_eq!(cfg.hooks.capacity, 1024);
    assert!(cfg.hooks.enabled);
    assert_eq!(cfg.skills.dir, "skills");
    assert!(cfg.skills.autoload);
    assert_eq!(cfg.variables.tar_dir, "TVStxt/tar");
    assert!(cfg.variables.hot_reload);
    assert_eq!(cfg.agents.dir, "agents");
    assert!(cfg.agents.single_agent_gate);
    assert!(!cfg.tools.block.enabled);
    assert!(cfg.tools.block.fallback_to_function_call);
    assert_eq!(cfg.telegram.webhook.public_url, "");
    assert!(!cfg.vector.tags.hierarchy_enabled);
    assert_eq!(cfg.vector.tags.max_depth, 6);
    assert_eq!(cfg.wstool.bind, "127.0.0.1:18790");
    assert_eq!(cfg.wstool.auth_token, "");
    assert!(!cfg.canvas.host_endpoint_enabled);
    assert_eq!(cfg.canvas.session_ttl_secs, 1800);
    assert_eq!(cfg.nodebridge.listen, "127.0.0.1:18788");

    cfg.validate()
        .expect("legacy-only config must pass validate() with defaults");
}

#[test]
fn full_v2_sections_round_trip_and_validate() {
    let cfg = parse(FULL_V2);

    // Round-trip: every explicit value must appear on the decoded struct.
    assert_eq!(cfg.hooks.capacity, 2048);
    assert!(cfg.hooks.enabled);
    assert_eq!(cfg.skills.dir, "skills");
    assert!(!cfg.skills.autoload);
    assert!(!cfg.variables.hot_reload);
    assert!(!cfg.agents.single_agent_gate);
    assert!(cfg.tools.block.enabled);
    assert!(!cfg.tools.block.fallback_to_function_call);
    assert_eq!(cfg.telegram.webhook.public_url, "https://example.test/tg");
    assert_eq!(cfg.telegram.webhook.secret_token, "s3cr3t");
    assert!(cfg.telegram.webhook.drop_updates_on_reconnect);
    assert!(cfg.vector.tags.hierarchy_enabled);
    assert_eq!(cfg.vector.tags.max_depth, 8);
    assert_eq!(cfg.wstool.bind, "127.0.0.1:18790");
    assert_eq!(cfg.wstool.heartbeat_secs, 30);
    assert!(cfg.canvas.host_endpoint_enabled);
    assert_eq!(cfg.canvas.session_ttl_secs, 900);
    assert_eq!(cfg.nodebridge.listen, "127.0.0.1:18788");

    cfg.validate().expect("full-v2 config must pass validate()");
}

#[test]
fn default_constructed_config_validates() {
    // The tightest back-compat smoke: a default config (the `Config::default`
    // value used by `load_from_path` on an empty file) validates.
    let cfg = Config::default();
    cfg.validate()
        .expect("Config::default() must pass validate()");
}
