"""Triage Worker: Classifies and routes incoming GitHub issues."""

import json
import os
from typing import Optional

# Labels mapped to classification keywords
LABEL_MAP = {
    "bug": ["bug", "error", "crash", "panic", "fails", "broken"],
    "enhancement": ["feature", "request", "want", "suggest", "idea"],
    "documentation": ["docs", "documentation", "readme", "typo"],
    "security": ["security", "vuln", "cve", "exploit"],
    "performance": ["slow", "perf", "latency", "optimize"],
}


def classify_issue(title: str, body: str) -> list[str]:
    """Classify an issue based on title and body keywords."""
    text = f"{title} {body}".lower()
    labels = []
    for label, keywords in LABEL_MAP.items():
        if any(kw in text for kw in keywords):
            labels.append(label)
    return labels if labels else ["triage"]


def estimate_effort(body: str) -> str:
    """Rough effort estimation from description length and complexity markers."""
    word_count = len(body.split())
    has_code = "```" in body
    has_steps = any(m in body for m in ["steps to reproduce", "expected behavior"])

    if word_count > 500 or (has_code and has_steps):
        return "complex"
    elif word_count > 100 or has_code:
        return "medium"
    return "simple"


def handler(event: dict, context=None) -> dict:
    """Main entry point for Cloud Run / Pub/Sub event."""
    issue = event.get("issue", event)
    title = issue.get("title", "")
    body = issue.get("body", "")

    labels = classify_issue(title, body)
    effort = estimate_effort(body)

    result = {
        "issue_number": issue.get("number"),
        "labels": labels,
        "effort": effort,
        "needs_human_review": effort == "complex",
    }

    return {"statusCode": 200, "body": json.dumps(result)}


if __name__ == "__main__":
    import sys
    test_event = json.loads(sys.stdin.read()) if not sys.stdin.isatty() else {}
    print(json.dumps(handler(test_event), indent=2))
