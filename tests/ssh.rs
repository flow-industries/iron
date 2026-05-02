#![allow(clippy::unwrap_used)]

use iron::ssh::build_upload_cmd;

#[test]
fn upload_cmd_uses_quoted_heredoc() {
    let cmd = build_upload_cmd("/tmp/x", "hello");
    assert!(cmd.starts_with("cat > /tmp/x <<'FLOW_EOF'\n"));
    assert!(cmd.ends_with("\nFLOW_EOF"));
}

#[test]
fn upload_cmd_preserves_single_quotes_verbatim() {
    let content = "f\"Job {conclusion or 'finished'}\"";
    let cmd = build_upload_cmd("/tmp/x", content);
    assert!(
        cmd.contains("'finished'"),
        "single quotes must be preserved verbatim inside quoted heredoc, got: {cmd}"
    );
    assert!(
        !cmd.contains("'\\''"),
        "must not shell-escape single quotes (heredoc is quoted): {cmd}"
    );
}

#[test]
fn upload_cmd_preserves_special_chars() {
    let content = "$VAR `cmd` \"quotes\" 'apostrophes' \\backslash";
    let cmd = build_upload_cmd("/tmp/x", content);
    assert!(cmd.contains(content));
}
