#![allow(clippy::unwrap_used)]

use iron::r2::{custom_domain_cname_target, s3_endpoint};

#[test]
fn s3_endpoint_uses_account_subdomain() {
    assert_eq!(
        s3_endpoint("abc123"),
        "https://abc123.r2.cloudflarestorage.com"
    );
}

#[test]
fn custom_domain_cname_target_format() {
    assert_eq!(
        custom_domain_cname_target("flow-media", "abc123"),
        "flow-media.abc123.r2.cloudflarestorage.com"
    );
}
