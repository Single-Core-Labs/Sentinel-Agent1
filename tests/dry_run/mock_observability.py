"""
Mock Grafana + Jaeger HTTP backends for dry-run observability testing.

Serves realistic responses for the subset of endpoints that
query_otel_traces and query_grafana_panel call.
"""

from __future__ import annotations

import json
import time

try:
    from http.server import HTTPServer, BaseHTTPRequestHandler
except ImportError:
    from http.server import HTTPServer, BaseHTTPRequestHandler


MOCK_DASHBOARD = {
    "dashboard": {
        "title": "Production Observability",
        "uid": "prod-obs-001",
        "panels": [
            {
                "id": 1,
                "title": "API Error Rate",
                "type": "timeseries",
                "datasource": {"type": "prometheus", "uid": "prometheus-prod"},
                "targets": [
                    {
                        "expr": 'sum(rate(http_requests_total{status=~"5.."}[5m])) / sum(rate(http_requests_total[5m])) * 100',
                        "legendFormat": "Error Rate",
                    }
                ],
            },
            {
                "id": 2,
                "title": "P99 Latency",
                "type": "timeseries",
                "datasource": {"type": "prometheus", "uid": "prometheus-prod"},
                "targets": [
                    {
                        "expr": 'histogram_quantile(0.99, sum(rate(http_request_duration_seconds_bucket[5m])) by (le))',
                        "legendFormat": "P99",
                    }
                ],
            },
            {
                "id": 3,
                "title": "CPU Utilization",
                "type": "gauge",
                "datasource": {"type": "prometheus", "uid": "prometheus-prod"},
                "targets": [
                    {
                        "expr": '100 - (avg(rate(node_cpu_seconds_total{mode="idle"}[5m])) * 100)',
                        "legendFormat": "CPU %",
                    }
                ],
            },
        ],
    },
    "meta": {"type": "db", "canSave": True, "canEdit": False},
}

MOCK_TRACES_RESPONSE = {
    "data": [
        {
            "traceID": "abc123def4567890",
            "spans": [
                {
                    "traceID": "abc123def4567890",
                    "spanID": "span-001",
                    "operationName": "POST /api/orders",
                    "serviceName": "order-service",
                    "startTime": int(time.time() * 1_000_000) - 5_000_000,
                    "duration": 1200000,
                    "kind": "SPAN_KIND_SERVER",
                    "status": {"code": 2, "message": "context deadline exceeded"},
                    "tags": [
                        {"key": "http.status_code", "type": "int64", "value": 504},
                        {"key": "error", "type": "bool", "value": True},
                    ],
                },
                {
                    "traceID": "abc123def4567890",
                    "spanID": "span-002",
                    "operationName": "SELECT FROM orders",
                    "serviceName": "order-service",
                    "startTime": int(time.time() * 1_000_000) - 5_000_000,
                    "duration": 1150000,
                    "kind": "SPAN_KIND_CLIENT",
                    "status": {"code": 2, "message": "connection timeout"},
                    "tags": [
                        {"key": "db.system", "type": "string", "value": "postgresql"},
                        {"key": "error", "type": "bool", "value": True},
                    ],
                },
            ],
        },
        {
            "traceID": "def456abc7890123",
            "spans": [
                {
                    "traceID": "def456abc7890123",
                    "spanID": "span-003",
                    "operationName": "GET /api/users",
                    "serviceName": "user-service",
                    "startTime": int(time.time() * 1_000_000) - 3_000_000,
                    "duration": 45000,
                    "kind": "SPAN_KIND_SERVER",
                    "status": {"code": 1},
                    "tags": [
                        {"key": "http.status_code", "type": "int64", "value": 200},
                    ],
                },
            ],
        },
    ]
}

MOCK_PROMQL_RESULT = {
    "status": "success",
    "data": {
        "resultType": "vector",
        "result": [
            {
                "metric": {"__name__": "http_requests_total", "status": "200", "method": "GET"},
                "value": [time.time(), "15234"],
            },
            {
                "metric": {"__name__": "http_requests_total", "status": "500", "method": "GET"},
                "value": [time.time(), "89"],
            },
            {
                "metric": {"__name__": "http_requests_total", "status": "504", "method": "POST"},
                "value": [time.time(), "23"],
            },
        ],
    },
}


class MockGrafanaHandler(BaseHTTPRequestHandler):
    """Simulates the Grafana HTTP API subset used by observability tools."""

    def _send_json(self, data, status=200):
        body = json.dumps(data).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        if self.path.startswith("/api/dashboards/uid/"):
            self._send_json(MOCK_DASHBOARD)
        elif self.path == "/api/user":
            self._send_json({"id": 1, "login": "mock-agent"})
        else:
            self._send_json({"error": "not found"}, 404)

    def do_POST(self):
        if "/api/v1/query_range" in self.path or "/api/v1/query" in self.path:
            self._send_json(MOCK_PROMQL_RESULT)
        else:
            self._send_json({"error": "not found"}, 404)

    def log_message(self, format, *args):
        pass


def run_mock_server(host="127.0.0.1", port=0):
    """Start mock server on a free port. Returns (server, port)."""
    server = HTTPServer((host, port), MockGrafanaHandler)
    port = server.server_address[1]
    return server, port
