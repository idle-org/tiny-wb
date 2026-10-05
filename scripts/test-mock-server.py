"""Smoke-test the built mock server CLI over loopback; run from the repo root.

Build with cargo build --no-default-features --features mock --example
mock-tinysa-server. Optionally pass an executable path (for CARGO_TARGET_DIR).
"""
import os
import socket
import subprocess
import sys

exe = sys.argv[1] if len(sys.argv) > 1 else 'target/debug/examples/mock-tinysa-server' + ('.exe' if os.name == 'nt' else '')
assert subprocess.run([exe, '--help'], capture_output=True, timeout=5).returncode == 0
assert subprocess.run([exe, '--unknown'], capture_output=True, timeout=5).returncode != 0
server = subprocess.Popen([exe, '--bind', '127.0.0.1:0', '--config', 'config/mock-basic.toml', '--once'], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
try:
    line = server.stdout.readline()
    address = line.split()[4]
    host, port = address.rsplit(':', 1)
    with socket.create_connection((host, int(port)), timeout=3) as client:
        def response():
            data = b''
            while not data.endswith(b'ch>'):
                block = client.recv(4096)
                assert block, 'unexpected EOF'
                data += block
            return data
        assert response() == b'ch>'
        client.sendall(b'version\r')
        assert b'tinySA_v1.4-mock' in response()
        client.sendall(b'mode high input\r')
        assert b'error:' not in response()
        client.sendall(b'scanraw 240000000 360000000 4\r')
        data = response()
        frame = data[data.index(b'{'):]
        assert frame[1] == ord('x') and frame[13] == ord('}')
    assert server.wait(timeout=5) == 0, server.stderr.read()
    print('Standalone mock server: CLI validation, dynamic port, synthetic identity, scanraw and --once lifecycle passed.')
finally:
    if server.poll() is None:
        server.kill()
        server.wait()
