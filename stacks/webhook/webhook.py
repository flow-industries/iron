import hashlib
import hmac
import json
import os
import sys
import urllib.error
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

SECRET = os.environ.get("GITHUB_WEBHOOK_SECRET", "").encode()
BIND = os.environ.get("BIND", "0.0.0.0:8080")
DISCORD_WEBHOOK_URL = os.environ.get("DISCORD_WEBHOOK_URL", "")
TELEGRAM_BOT_TOKEN = os.environ.get("TELEGRAM_BOT_TOKEN", "")
TELEGRAM_CHAT_ID = os.environ.get("TELEGRAM_CHAT_ID", "")

DISCORD_COLOR = {
    "info": 0x3498DB,
    "success": 0x2ECC71,
    "failure": 0xE74C3C,
}

TELEGRAM_EMOJI = {
    "info": "ℹ️",
    "success": "✅",
    "failure": "❌",
}

FAILURE_CONCLUSIONS = {"failure", "timed_out", "cancelled", "action_required"}


def log(*args):
    print(*args, file=sys.stderr, flush=True)


def post_json(url, payload, timeout=10):
    body = json.dumps(payload).encode()
    req = urllib.request.Request(
        url,
        data=body,
        headers={"Content-Type": "application/json"},
        method="POST",
    )
    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            resp.read()
    except (urllib.error.URLError, urllib.error.HTTPError, TimeoutError) as e:
        log(f"notify failed: {e}")


def notify(level, title, description):
    if DISCORD_WEBHOOK_URL:
        post_json(
            DISCORD_WEBHOOK_URL,
            {
                "embeds": [
                    {
                        "title": title,
                        "description": description,
                        "color": DISCORD_COLOR.get(level, DISCORD_COLOR["info"]),
                    }
                ]
            },
        )

    if TELEGRAM_BOT_TOKEN and TELEGRAM_CHAT_ID:
        emoji = TELEGRAM_EMOJI.get(level, TELEGRAM_EMOJI["info"])
        text = f"{emoji} <b>{title}</b>\n{description}"
        post_json(
            f"https://api.telegram.org/bot{TELEGRAM_BOT_TOKEN}/sendMessage",
            {
                "chat_id": TELEGRAM_CHAT_ID,
                "text": text,
                "parse_mode": "HTML",
            },
        )


def verify_signature(body, signature_header):
    if not SECRET:
        return False
    if not signature_header or not signature_header.startswith("sha256="):
        return False
    expected = "sha256=" + hmac.new(SECRET, body, hashlib.sha256).hexdigest()
    return hmac.compare_digest(expected, signature_header)


def has_self_hosted_label(workflow_job):
    labels = workflow_job.get("labels") or []
    return any(label == "self-hosted" for label in labels)


def handle_workflow_job(payload):
    job = payload.get("workflow_job") or {}
    if not has_self_hosted_label(job):
        return None

    action = payload.get("action")
    workflow_name = job.get("workflow_name") or "workflow"
    job_name = job.get("name") or "job"
    runner_name = job.get("runner_name") or "?"
    labels = ", ".join(job.get("labels") or [])
    head = f"{workflow_name} / {job_name}"

    if action == "queued":
        return ("info", f"Job queued: {head}", f"labels: {labels}")
    if action == "in_progress":
        return ("info", f"Job started: {head}", f"on runner-{runner_name}")
    if action == "completed":
        conclusion = (job.get("conclusion") or "").lower()
        if conclusion == "success":
            return (
                "success",
                f"Job done: {head}",
                f"on runner-{runner_name}",
            )
        if conclusion in FAILURE_CONCLUSIONS:
            return (
                "failure",
                f"Job {conclusion}: {head}",
                f"on runner-{runner_name}",
            )
        final = conclusion or "finished"
        return (
            "info",
            f"Job {final}: {head}",
            f"on runner-{runner_name}",
        )
    return None


class Handler(BaseHTTPRequestHandler):
    def log_message(self, fmt, *args):
        log(self.address_string(), fmt % args)

    def respond(self, code, body=b""):
        self.send_response(code)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        if body:
            self.wfile.write(body)

    def do_GET(self):
        if self.path == "/health":
            self.respond(200, b"ok\n")
            return
        self.respond(404)

    def do_POST(self):
        if self.path != "/github":
            self.respond(404)
            return

        length = int(self.headers.get("Content-Length", "0") or 0)
        body = self.rfile.read(length) if length else b""

        signature = self.headers.get("X-Hub-Signature-256", "")
        if not verify_signature(body, signature):
            self.respond(401, b"invalid signature\n")
            return

        event = self.headers.get("X-GitHub-Event", "")
        if event != "workflow_job":
            self.respond(204)
            return

        try:
            payload = json.loads(body)
        except json.JSONDecodeError:
            self.respond(400, b"invalid json\n")
            return

        result = handle_workflow_job(payload)
        if result is not None:
            level, title, description = result
            notify(level, title, description)

        self.respond(202)


def main():
    if not SECRET:
        log("FATAL: GITHUB_WEBHOOK_SECRET not set")
        sys.exit(1)
    host, _, port = BIND.partition(":")
    server = ThreadingHTTPServer((host, int(port or "8080")), Handler)
    log(f"webhook listening on {host}:{port}")
    server.serve_forever()


if __name__ == "__main__":
    main()
