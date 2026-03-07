#![allow(clippy::unwrap_used)]

use iron::cloudflare::extract_zone;

#[test]
fn test_extract_zone() {
    assert_eq!(extract_zone("flow.industries"), "flow.industries");
    assert_eq!(extract_zone("id.flow.industries"), "flow.industries");
    assert_eq!(extract_zone("flow.talk"), "flow.talk");
    assert_eq!(extract_zone("eu.play.flow.game"), "flow.game");
}
