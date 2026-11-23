"""Exercise the installed Core's parent pipe using a synthetic owner session."""

import os
from pathlib import Path
import subprocess
import sys
import time
import urllib.request


def main() -> None:
    executable, data_dir = sys.argv[1:]
    data = Path(data_dir)
    data.mkdir(parents=True, exist_ok=True)
    (data / ".macos-owner-session").touch()
    token = "a" * 64
    env = dict(os.environ, JARVIS_DATA_DIR=data_dir, JARVIS_TOKEN=token, JARVIS_PORT="18766")
    offline = subprocess.run([executable], env=env, capture_output=True, check=False, timeout=10)
    assert offline.returncode == 2 and b"sblocca" in offline.stderr, (offline.returncode, offline.stderr)
    env.update(JARVIS_MACOS_SESSION="1", JARVIS_EXIT_WITH_PARENT="stdin")
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
            child.stdin.close()
            assert child.wait(timeout=15) == 0
        finally:
            if child.poll() is None:
                child.kill()


if __name__ == "__main__":
    main()
