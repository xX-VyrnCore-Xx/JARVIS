"""Exercise the installed Core's parent pipe using a synthetic owner session."""

import json
import os
from pathlib import Path
import subprocess
import sys
import time
import urllib.error
import urllib.request


def action(path, body=None, *, token, remote=False):
    headers = {"content-type": "application/json"}
    headers["x-jarvis-device" if remote else "x-jarvis-token"] = token
    if remote:
        headers["host"] = "fixture.example.invalid:18766"
    request = urllib.request.Request("http://127.0.0.1:18766" + path,
                                     data=json.dumps(body).encode() if body is not None else None,
                                     headers=headers)
    try:
        with urllib.request.urlopen(request, timeout=10) as response:
            return response.status, json.load(response)
    except urllib.error.HTTPError as error:
        return error.code, json.load(error)


def main() -> None:
    executable, data_dir = sys.argv[1:]
    data = Path(data_dir)
    data.mkdir(parents=True, exist_ok=True)
    (data / ".macos-owner-session").touch()
    token = "a" * 64
    env = dict(os.environ, JARVIS_DATA_DIR=data_dir, JARVIS_TOKEN=token, JARVIS_PORT="18766")
    offline = subprocess.run([executable], env=env, capture_output=True, check=False, timeout=10)
    assert offline.returncode == 2 and b"sblocca" in offline.stderr, (offline.returncode, offline.stderr)
    env.update(JARVIS_MACOS_SESSION="1", JARVIS_EXIT_WITH_PARENT="stdin",
               JARVIS_ALLOWED_HOSTS="fixture.example.invalid")
    denied = subprocess.run([executable], env=env, input=b"x" * 64 + b"\n", capture_output=True,
                            check=False, timeout=10)
    assert denied.returncode == 2
    with subprocess.Popen([executable], env=env, stdin=subprocess.PIPE) as child:
        try:
            assert child.stdin is not None
            child.stdin.write(token.encode() + b"\n")
            child.stdin.flush()
            request = urllib.request.Request("http://127.0.0.1:18766/api/tools",
                                             headers={"X-Jarvis-Token": token})
            deadline = time.monotonic() + 30
            while True:
                try:
                    with urllib.request.urlopen(request, timeout=1) as response:
                        assert response.status == 200
                    break
                except OSError:
                    if child.poll() is not None or time.monotonic() >= deadline:
                        raise
                    time.sleep(0.25)
            status, invite = action("/api/devices/pairing", {"scopes": ["chat", "confirm"]}, token=token)
            assert status == 200, invite
            status, pair = action("/api/devices/pair", {"code": invite["code"], "name": "CI", "platform": "android"},
                                  token="", remote=True)
            assert status == 200, pair
            device = pair["token"]
            status, task = action("/api/chat", {"message": "scrivi esempio.txt: fixture"}, token=device, remote=True)
            assert status == 200 and task["confirmation"]["risk"] == "high", task
            path = "/api/confirmations/" + task["confirmation"]["confirmation_id"]
            status, result = action(path, {"approve": True}, token=device, remote=True)
            assert status == 403, result
            status, result = action(path, {"approve": True}, token=token)
            assert status == 200 and result["task"]["state"] == "succeeded", result
            child.stdin.close()
            assert child.wait(timeout=15) == 0
        finally:
            if child.poll() is None:
                child.kill()


if __name__ == "__main__":
    main()
