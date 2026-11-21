"""Verifica selezione SemVer e comandi di retention delle release."""

import sys
import unittest
from pathlib import Path
from types import SimpleNamespace

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT))

from scripts.prune_releases import newest_tag, prune_releases, version_key  # noqa: E402


class ReleaseCleanup(unittest.TestCase):
    def test_selects_highest_semver_with_semver_prerelease_order(self):
        tags = [
            "v1.13.9",
            "v1.13.10",
            "v1.14.0-rc.2",
            "v1.14.0-rc.10",
            "v1.14.0",
        ]
        self.assertEqual(newest_tag(tags), "v1.14.0")
        self.assertGreater(version_key("v1.14.0-rc.10"), version_key("v1.14.0-rc.2"))

    def test_prunes_old_and_non_semver_releases_and_tags(self):
        tags = ["release-legacy", "v1.9.9", "v1.10.0"]
        calls = []

        def fake_run(command, **kwargs):
            calls.append((command, kwargs))
            if command[:3] == ["gh", "release", "list"]:
                return SimpleNamespace(stdout="\n".join(tags))
            return SimpleNamespace(stdout="")

        kept = prune_releases("owner/repo", run=fake_run)

        self.assertEqual(kept, "v1.10.0")
        self.assertEqual(
            [call[0][3] for call in calls[1:]],
            ["release-legacy", "v1.9.9"],
        )
        self.assertTrue(all(call[0][5] == "--cleanup-tag" for call in calls[1:]))

    def test_refuses_to_delete_when_no_valid_release_tag_exists(self):
        with self.assertRaisesRegex(ValueError, "SemVer valido"):
            newest_tag(["legacy", "v01.2.3"])


if __name__ == "__main__":
    unittest.main()
