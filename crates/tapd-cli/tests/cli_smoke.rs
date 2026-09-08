use std::{
    fs,
    path::PathBuf,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use serde_json::Value;

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_tapd-cli")
}

fn fixture_config() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/config.toml")
}

fn fixture_image() -> PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let directory =
        std::env::temp_dir().join(format!("tapd-cli-smoke-{}-{suffix}", std::process::id()));
    fs::create_dir(&directory).expect("create image fixture directory");
    let path = directory.join("overview.png");
    fs::write(&path, b"local image fixture").expect("write image fixture");
    path
}

fn run(args: &[&str]) -> std::process::Output {
    Command::new(binary()).args(args).output().expect("run CLI")
}

#[test]
fn help_uses_the_final_binary_name() {
    let output = run(&["--help"]);
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("UTF-8 help");
    assert!(stdout.contains("Usage: tapd-cli"));
    #[cfg(feature = "demo")]
    assert!(stdout.contains("  demo "));
    #[cfg(not(feature = "demo"))]
    assert!(!stdout.contains("  demo "));
}

#[test]
fn auth_commands_expose_keychain_workflow_without_plaintext_token_argument() {
    let output = run(&["auth", "--help"]);
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("UTF-8 auth help");
    for command in ["login", "status", "logout", "test", "whoami"] {
        assert!(stdout.contains(command));
    }

    let output = run(&["auth", "login", "--help"]);
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("UTF-8 login help");
    assert!(stdout.contains("--from-env"));
    assert!(!stdout.contains("--token"));
}

#[test]
fn bundled_skill_can_be_listed_and_read_without_config_or_network() {
    let output = run(&["skill", "list"]);
    assert!(output.status.success());
    let response: Value = serde_json::from_slice(&output.stdout).expect("JSON output");
    assert_eq!(response["data"]["skill"], "tapd-cli");
    assert!(
        response["data"]["files"]
            .as_array()
            .expect("embedded files")
            .contains(&Value::String("SKILL.md".into()))
    );

    let output = run(&["skill", "read", "references/stories.md"]);
    assert!(output.status.success());
    let contents = String::from_utf8(output.stdout).expect("UTF-8 Skill reference");
    assert!(contents.contains("tapd-cli story list"));
}

#[test]
fn story_inline_image_dry_run_plans_upload_and_renders_image_element() {
    let config = fixture_config();
    let image = fixture_image();
    let output = Command::new(binary())
        .args([
            "--config",
            config.to_str().expect("UTF-8 config path"),
            "--dry-run",
            "story",
            "create",
            "--name",
            "性能监控图表展示",
            "--description",
            "## 数据概览\n\n![概览]({{tapd-image:overview}})",
            "--image",
            &format!("overview={}", image.display()),
        ])
        .output()
        .expect("run inline image dry-run");
    fs::remove_file(&image).expect("remove image fixture");
    fs::remove_dir(image.parent().expect("fixture parent")).expect("remove fixture directory");
    assert!(output.status.success());
    let response: Value = serde_json::from_slice(&output.stdout).expect("JSON output");
    let description = response["data"]["params"]
        .as_array()
        .expect("params")
        .iter()
        .find_map(|pair| {
            let pair = pair.as_array()?;
            (pair.first()?.as_str()? == "description")
                .then(|| pair.get(1)?.as_str())
                .flatten()
        })
        .expect("description parameter");
    assert!(description.contains("<img src=\"tapd-image-pending://overview\""));
    assert_eq!(
        response["data"]["extra"]["inline_images"][0]["upload"]["endpoint"],
        "/files/upload_image"
    );
    assert_eq!(
        response["data"]["extra"]["inline_images"][0]["read_back"]["endpoint"],
        "/files/get_image"
    );
}

#[test]
fn story_write_dry_run_has_only_user_business_content() {
    let config = fixture_config();
    let output = Command::new(binary())
        .args([
            "--config",
            config.to_str().expect("UTF-8 config path"),
            "--dry-run",
            "story",
            "create",
            "--name",
            "性能监控数据完整性核对",
            "--description",
            "核对近期性能监控数据的完整性。",
            "--description-format",
            "plain",
        ])
        .output()
        .expect("run dry-run create");
    assert!(output.status.success());
    let response: Value = serde_json::from_slice(&output.stdout).expect("JSON output");
    let params = response["data"]["params"]
        .as_array()
        .expect("dry-run params");
    let values = params
        .iter()
        .filter_map(|pair| pair.as_array()?.get(1)?.as_str())
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();
    for forbidden in ["tapd-cli", "自动化", "测试", "runid", "run_id"] {
        assert!(!values.contains(forbidden));
    }
}

#[test]
fn url_parser_is_available_without_a_config_or_token() {
    let output = run(&[
        "url",
        "parse",
        "https://www.tapd.cn/tapd_fe/37916943/story/detail/1137916943001018636",
    ]);
    assert!(output.status.success());
    let response: Value = serde_json::from_slice(&output.stdout).expect("JSON output");
    assert_eq!(response["data"]["workspace_id"], "37916943");
    assert_eq!(response["data"]["resource"], "story");
}

#[cfg(feature = "demo")]
#[test]
fn demo_request_plan_resolves_fixed_domain_configuration_offline() {
    let config = fixture_config();
    let output = Command::new(binary())
        .args([
            "--config",
            config.to_str().expect("UTF-8 config path"),
            "demo",
            "request",
            "plan",
            "--subject",
            "Reporting",
            "--summary",
            "Add a monthly view",
        ])
        .output()
        .expect("run demo plan");
    assert!(output.status.success());
    let response: Value = serde_json::from_slice(&output.stdout).expect("JSON output");
    assert_eq!(response["data"]["organization"], "demo");
    assert_eq!(response["data"]["domain"], "request");
    assert_eq!(response["data"]["workspace_id"], "37916943");
    assert!(
        response["data"]["params"]
            .as_array()
            .expect("params")
            .contains(&serde_json::json!(["category_id", "7"]))
    );
}
