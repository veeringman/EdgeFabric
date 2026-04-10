#!/usr/bin/env python3
"""Test WebSocket streaming endpoint - writes results to /tmp/ws_test.log"""
import asyncio, json, sys, os

os.environ.setdefault('TERM', 'dumb')

async def test():
    try:
        import websockets
    except ImportError:
        import subprocess
        subprocess.check_call([sys.executable, '-m', 'pip', 'install', 'websockets', '-q'])
        import websockets

    log = open('/tmp/ws_test.log', 'w')
    def p(s):
        log.write(s + '\n')
        log.flush()

    device_id = sys.argv[1] if len(sys.argv) > 1 else "c3ac380b-8f4e-4657-9f75-a0a46aa510b2"
    uri = f"ws://127.0.0.1:6060/api/v1/devices/{device_id}/stream"
    p(f"Connecting to {uri}")
    try:
        async with websockets.connect(uri, open_timeout=5) as ws:
            p("Connected!")
            for i in range(5):
                try:
                    msg = await asyncio.wait_for(ws.recv(), timeout=3)
                    if isinstance(msg, str):
                        p(f"  Text[{i}]: {msg[:300]}")
                    else:
                        p(f"  Binary[{i}]: {len(msg)} bytes")
                except asyncio.TimeoutError:
                    p(f"  Timeout waiting for msg {i}")
                    break
    except Exception as e:
        p(f"Error: {type(e).__name__}: {e}")
    log.close()

asyncio.run(test())
