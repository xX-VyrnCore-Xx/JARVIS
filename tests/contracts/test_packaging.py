"""Verifica riproducibilità e struttura dei pacchetti di release."""

import hashlib
import os
from pathlib import Path
import subprocess
import sys
import tarfile
from tempfile import TemporaryDirectory
import unittest
import zipfile

ROOT = Path(__file__).resolve().parents[2]


class Packaging(unittest.TestCase):
    def test_zip_and_tar_are_independent_of_file_metadata(self):
        for target, suffix in (("windows-x86_64", ".zip"), ("macos-aarch64", ".tar.gz"), ("linux-x86_64", ".tar.gz")):
            with self.subTest(target=target), TemporaryDirectory() as tmp:
                binary = Path(tmp) / "core"
                binary.write_bytes(b"fixture binaria")
                packages = []
                for run, mode in enumerate((0o600, 0o755)):
                    os.chmod(binary, mode)
                    timestamp = 1700000000 + run * 3600
                    os.utime(binary, (timestamp, timestamp))
                    output = Path(tmp) / str(run)
                    subprocess.run(
                        [sys.executable, str(ROOT / "scripts/package.py"), "--binary", str(binary),
                         "--target", target, "--out", str(output)],
                        check=True, capture_output=True,
                    )
                    packages.append(next(output.glob(f"*{suffix}")))
                first, second = packages
                self.assertEqual(first.read_bytes(), second.read_bytes())
                digest = hashlib.sha256(first.read_bytes()).hexdigest()
                self.assertEqual(first.with_name(first.name + ".sha256").read_text().split(), [digest, first.name])
                prefix = first.name.removesuffix(suffix)
                if suffix == ".zip":
                    with zipfile.ZipFile(first) as archive:
                        self.assertEqual(archive.read(f"{prefix}/bin/jarvis.exe"), binary.read_bytes())
                        self.assertIn(f"{prefix}/jarvis.cmd", archive.namelist())
                else:
                    with tarfile.open(first) as archive:
                        executable = archive.getmember(f"{prefix}/bin/jarvis")
                        self.assertEqual(executable.mode, 0o755)
                        self.assertEqual(archive.getmember(f"{prefix}/jarvis.sh").mode, 0o755)
                        with archive.extractfile(executable) as content:
                            self.assertEqual(content.read(), binary.read_bytes())
