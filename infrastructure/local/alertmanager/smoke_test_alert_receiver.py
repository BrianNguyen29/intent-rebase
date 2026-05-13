#!/usr/bin/env python3
"""
Local-only smoke test for Alertmanager -> alert-receiver delivery.

Posts a bounded test alert to the local Alertmanager API and prints
commands to inspect alert-receiver logs and clean up.

Usage:
    python3 infrastructure/local/alertmanager/smoke_test_alert_receiver.py

Requirements:
    - Alertmanager must be reachable at http://localhost:9093
    - alert-receiver must be running (standalone or via docker compose)
"""

import json
import sys
import urllib.error
import urllib.request


ALERTMANAGER_URL = "http://localhost:9093/api/v1/alerts"


def main():
    test_alert = [
        {
            "labels": {
                "alertname": "TestAlert",
                "severity": "warning",
                "slo": "propagation",
                "instance": "smoke-test",
                "source": "local-smoke-helper",
            },
            "annotations": {
                "summary": "Smoke test alert for local alert-receiver",
                "description": "This is a bounded local-only test alert.",
            },
            "generatorURL": "http://localhost/smoke-test",
        }
    ]

    req = urllib.request.Request(
        ALERTMANAGER_URL,
        data=json.dumps(test_alert).encode("utf-8"),
        headers={"Content-Type": "application/json"},
        method="POST",
    )

    try:
        with urllib.request.urlopen(req, timeout=10) as resp:
            body = resp.read().decode("utf-8", errors="replace")
            print(f"Alertmanager responded: {resp.status} {body}")
    except urllib.error.HTTPError as e:
        print(f"Alertmanager returned HTTP error: {e.code} {e.reason}", file=sys.stderr)
        body = e.read().decode("utf-8", errors="replace")
        print(body, file=sys.stderr)
        sys.exit(1)
    except urllib.error.URLError as e:
        print(f"Failed to reach Alertmanager: {e.reason}", file=sys.stderr)
        sys.exit(1)

    print("\nNext steps:")
    print("1. Inspect alert-receiver logs for the TestAlert payload:")
    print("   docker compose -f infrastructure/local/docker-compose.yml --profile observability logs alert-receiver")
    print("\n2. Or if running standalone:")
    print("   # Check the terminal where webhook_receiver.py is running")
    print("\n3. Stop only alert-receiver and alertmanager:")
    print("   docker compose -f infrastructure/local/docker-compose.yml --profile observability stop alert-receiver alertmanager")
    print("   docker compose -f infrastructure/local/docker-compose.yml --profile observability rm -f alert-receiver alertmanager")
    print("\n4. To stop the whole observability profile, including Grafana/Prometheus if running:")
    print("   docker compose -f infrastructure/local/docker-compose.yml --profile observability down")


if __name__ == "__main__":
    main()
