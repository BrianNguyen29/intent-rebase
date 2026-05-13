#!/usr/bin/env python3
"""
Local-only Alertmanager webhook receiver for manual alert inspection.

This is a lightweight, local-dev helper. It is NOT production-ready,
does not persist alerts, and does not route to real Slack/PagerDuty/email.
Use it to see alert payloads when running local Prometheus + Alertmanager.

Usage:
    python3 infrastructure/local/alertmanager/webhook_receiver.py
    # Then trigger an alert in local Prometheus and watch stdout.

Standalone use listens at http://localhost:9094/webhook. Docker Compose
Alertmanager routes to the receiver service at http://alert-receiver:9094/webhook.
"""

import json
import sys
from http.server import BaseHTTPRequestHandler, HTTPServer


class AlertHandler(BaseHTTPRequestHandler):
    def do_POST(self):
        content_length = int(self.headers.get("Content-Length", 0))
        body = self.rfile.read(content_length)
        try:
            payload = json.loads(body)
        except json.JSONDecodeError:
            payload = {"raw": body.decode("utf-8", errors="replace")}

        print("--- Alertmanager Webhook ---", flush=True)
        print(json.dumps(payload, indent=2), flush=True)
        print("----------------------------\n", flush=True)

        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.end_headers()
        self.wfile.write(b'{"status":"ok"}')

    def log_message(self, format, *args):
        # Suppress default access logs; we print alert payloads above.
        pass


def main():
    port = 9094
    server = HTTPServer(("", port), AlertHandler)
    print(f"Local Alertmanager webhook receiver listening on http://localhost:{port}/webhook", flush=True)
    print("Press Ctrl+C to stop.\n", flush=True)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        print("\nShutting down.", flush=True)
        server.shutdown()
        sys.exit(0)


if __name__ == "__main__":
    main()
