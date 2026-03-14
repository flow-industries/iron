use iron::ghcr::{format_relative_time, parse_ghcr_image, select_tag};

#[test]
fn parse_standard_ghcr_image() {
    let result = parse_ghcr_image("ghcr.io/flow-industries/auth:latest");
    assert_eq!(result, Some(("flow-industries", "auth")));
}

#[test]
fn parse_ghcr_image_without_tag() {
    let result = parse_ghcr_image("ghcr.io/flow-industries/auth");
    assert_eq!(result, Some(("flow-industries", "auth")));
}

#[test]
fn parse_ghcr_image_with_version_tag() {
    let result = parse_ghcr_image("ghcr.io/myorg/myapp:v1.2.3");
    assert_eq!(result, Some(("myorg", "myapp")));
}

#[test]
fn parse_docker_hub_image_returns_none() {
    assert_eq!(parse_ghcr_image("postgres:17"), None);
}

#[test]
fn parse_third_party_image_returns_none() {
    assert_eq!(
        parse_ghcr_image("prodrigestivill/postgres-backup-local"),
        None
    );
}

#[test]
fn parse_empty_string_returns_none() {
    assert_eq!(parse_ghcr_image(""), None);
}

#[test]
fn parse_ghcr_missing_package_returns_none() {
    assert_eq!(parse_ghcr_image("ghcr.io/owner/"), None);
}

#[test]
fn parse_ghcr_missing_owner_returns_none() {
    assert_eq!(parse_ghcr_image("ghcr.io/"), None);
}

#[test]
fn select_tag_picks_version_over_latest() {
    let tags = vec![
        "latest".to_string(),
        "v1.4.2".to_string(),
        "main".to_string(),
    ];
    assert_eq!(select_tag(&tags), "v1.4.2");
}

#[test]
fn select_tag_skips_main_and_master() {
    let tags = vec!["main".to_string(), "master".to_string(), "v2.0".to_string()];
    assert_eq!(select_tag(&tags), "v2.0");
}

#[test]
fn select_tag_shows_short_sha_when_no_version() {
    let tags = vec![
        "latest".to_string(),
        "b202c507776d37edba92f87ada6f59ba336257d8".to_string(),
    ];
    assert_eq!(select_tag(&tags), "b202c50");
}

#[test]
fn select_tag_falls_back_to_latest() {
    let tags = vec!["latest".to_string()];
    assert_eq!(select_tag(&tags), "latest");
}

#[test]
fn select_tag_empty_falls_back_to_latest() {
    let tags: Vec<String> = vec![];
    assert_eq!(select_tag(&tags), "latest");
}

#[test]
fn format_recent_timestamp() {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let three_hours_ago = now - 3 * 3600;

    let remaining = three_hours_ago % 3600;
    let day_seconds = three_hours_ago % 86400;
    let total_days = three_hours_ago / 86400;

    let mut year = 1970u32;
    let mut remaining_days = total_days;
    loop {
        let days_in_year: u64 = if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 {
            366
        } else {
            365
        };
        if remaining_days < days_in_year {
            break;
        }
        remaining_days -= days_in_year;
        year += 1;
    }

    let month_days = [31u64, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let is_leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    let mut month = 1u32;
    for (i, &md) in month_days.iter().enumerate() {
        let days = if i == 1 && is_leap { md + 1 } else { md };
        if remaining_days < days {
            break;
        }
        remaining_days -= days;
        month += 1;
    }
    let day = remaining_days + 1;
    let hour_of_day = day_seconds / 3600;
    let minute = (remaining % 3600) / 60;
    let second = remaining % 60;

    let ts = format!("{year:04}-{month:02}-{day:02}T{hour_of_day:02}:{minute:02}:{second:02}Z",);
    assert_eq!(format_relative_time(&ts), "3h ago");
}

#[test]
fn format_invalid_timestamp() {
    assert_eq!(format_relative_time("not-a-date"), "—");
}

#[test]
fn format_known_timestamp() {
    let result = format_relative_time("2020-01-01T00:00:00Z");
    assert!(
        result.ends_with("ago"),
        "Expected relative time, got: {result}"
    );
}
