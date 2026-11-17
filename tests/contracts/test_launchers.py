"""Vincoli sui lanciatori: Windows PowerShell 5.1 legge gli script senza BOM come cp1252."""

import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


class LauncherEncoding(unittest.TestCase):
    def test_windows_launchers_are_ascii(self):
        for name in ("jarvis.ps1", "jarvis.cmd"):
            data = (ROOT / name).read_bytes()
            bad = sorted({b for b in data if b > 127})
            self.assertFalse(bad, f"{name} contiene byte non ASCII {bad[:5]}: PowerShell 5.1 lo leggerebbe come cp1252")


if __name__ == "__main__":
    unittest.main()
