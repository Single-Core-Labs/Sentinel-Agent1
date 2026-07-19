#!/usr/bin/env python3
"""Mock WebSocket API server for testing the Sentinel agent CLI.

Serves pre-scripted responses over WebSocket to simulate a backend
analytics or event endpoint during development and integration tests.
"""

import argparse
import asyncio
import json
import sys
import time
from typing import Any

try:
    import websockets
except ImportError:
    print("ERROR: install websockets: pip install websockets", file=sys.stderr)
    sys.exit(1)

SCRIPT: list[dict[str, Any]] = [
    {"type": "session.created", "session_id": "mock-session-001"},
    {"type": "turn.started", "turn_id": "mock-turn-001", "thread_id": "mock-thread-001"},
    {"type": "model.request", "model": "mock-model", "prompt_tokens": 42},
    {"type": "model.response", "model": "mock-model", "completion_tokens": 128},
    {"type": "tool.call", "tool_name": "mock_tool", "duration_ms": 150},
    {"type": "turn.ended", "turn_id": "mock-turn-001", "tokens_used": 170, "duration_ms": 1200},
    {"type": "session.ended", "session_id": "mock-session-001"},
]

INTERVAL = 0.5  # seconds between events


async def handler(websocket):
    print(f"[mock-ws] Client connected: {websocket.remote_address}")
    try:
        # Wait for a client message first
        msg = await asyncio.wait_for(websocket.recv(), timeout=10)
        print(f"[mock-ws] Received: {msg}")

        # Replay the scripted events
        for event in SCRIPT:
            await websocket.send(json.dumps(event))
            print(f"[mock-ws] Sent: {event['type']}")
            await asyncio.sleep(INTERVAL)

        # Wait for close
        async for _ in websocket:
            pass
    except asyncio.TimeoutError:
        print("[mock-ws] No message received within timeout")
    except websockets.exceptions.ConnectionClosed:
        print("[mock-ws] Client disconnected")


async def main() -> None:
    parser = argparse.ArgumentParser(description="Mock WebSocket server")
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default="8765")
    args = parser.parse_args()

    print(f"[mock-ws] Starting on ws://{args.host}:{args.port}")
    async with websockets.serve(handler, args.host, args.port):
        await asyncio.Future()


if __name__ == "__main__":
    asyncio.run(main())
