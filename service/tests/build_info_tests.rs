use tinycongress_api::build_info::BuildInfoProvider;

#[test]
fn uses_env_values_when_provided() {
    let provider = BuildInfoProvider::from_lookup(|key| match key {
        "APP_VERSION" => Some("1.2.3".to_string()),
        "GIT_SHA" => Some("abc123".to_string()),
        "BUILD_TIME" => Some("2024-01-02T03:04:05Z".to_string()),
        "BUILD_MESSAGE" => Some("hello".to_string()),
        _ => None,
    });

    let info = provider.build_info();
    assert_eq!(info.version, "1.2.3");
    assert_eq!(info.git_sha, "abc123");
    assert_eq!(info.build_time, "2024-01-02T03:04:05+00:00");
    assert_eq!(info.message.as_deref(), Some("hello"));
}

#[test]
fn falls_back_when_env_missing() {
    let provider = BuildInfoProvider::from_lookup(|_| None);

    let info = provider.build_info();
    assert_eq!(info.version, "dev");
    assert_eq!(info.git_sha, "unknown");
    assert_eq!(info.build_time, "unknown");
    assert!(info.message.is_none());
}

#[test]
fn accepts_version_alias_when_app_version_absent() {
    let provider = BuildInfoProvider::from_lookup(|key| match key {
        "VERSION" => Some("0.9.0".to_string()),
        _ => None,
    });

    let info = provider.build_info();
    assert_eq!(info.version, "0.9.0");
}

#[test]
fn invalid_build_time_defaults_to_unknown() {
    let provider = BuildInfoProvider::from_lookup(|key| match key {
        "APP_VERSION" => Some("1.0.0".to_string()),
        "GIT_SHA" => Some("deadbeef".to_string()),
        "BUILD_TIME" => Some("not-a-date".to_string()),
        _ => None,
    });

    let info = provider.build_info();
    assert_eq!(info.build_time, "unknown");
}
