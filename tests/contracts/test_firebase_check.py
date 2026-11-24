"""scripts/check_firebase.py: riconosce configurazioni corrette e sbagliate senza stampare segreti."""

import json
import os
import subprocess
import sys
import tempfile
import unittest
import zipfile
from pathlib import Path

SCRIPT = Path(__file__).resolve().parents[2] / "scripts" / "check_firebase.py"
API_KEY = "AIza" + "B" * 35
APP_ID = "1:123456789012:android:0123456789abcdef"


def run(*args: str) -> subprocess.CompletedProcess:
    return subprocess.run([sys.executable, "-I", str(SCRIPT), *args], capture_output=True, text=True, timeout=30)


class FirebaseCheckTest(unittest.TestCase):
    def setUp(self):
        self.dir = Path(tempfile.mkdtemp(prefix="jarvis-fb-"))

    def write(self, name: str, data) -> str:
        path = self.dir / name
        path.write_text(json.dumps(data), encoding="utf-8")
        path.chmod(0o600)
        return str(path)

    def services(self, package="local.jarvis.app", project="jarvis-demo-1") -> str:
        return self.write("google-services.json", {
            "project_info": {"project_id": project, "project_number": "123456789012"},
            "client": [{"client_info": {"mobilesdk_app_id": APP_ID, "android_client_info": {"package_name": package}},
                        "api_key": [{"current_key": API_KEY}]}]})

    def account(self, project="jarvis-demo-1", kind="service_account") -> str:
        return self.write("sa.json", {"type": kind, "project_id": project, "client_email": "x@y.iam.gserviceaccount.com",
                                      "private_key": "-----BEGIN PRIVATE KEY-----\nabc\n-----END PRIVATE KEY-----\n",
                                      "private_key_id": "k1"})

    def test_a_consistent_configuration_passes_without_printing_secrets(self):
        result = run("--google-services", self.services(), "--service-account", self.account())
        self.assertEqual(result.returncode, 0, result.stdout)
        self.assertNotIn(API_KEY, result.stdout + result.stderr)
        self.assertNotIn("BEGIN PRIVATE KEY", result.stdout + result.stderr)

    def test_wrong_package_and_mismatched_projects_are_reported(self):
        wrong_package = run("--google-services", self.services(package="com.altro.app"))
        self.assertEqual(wrong_package.returncode, 1)
        self.assertIn("local.jarvis.app", wrong_package.stdout)
        mismatch = run("--google-services", self.services(), "--service-account", self.account(project="altro"))
        self.assertEqual(mismatch.returncode, 1)
        self.assertIn("non coincide", mismatch.stdout)

    def test_a_non_service_account_key_is_rejected(self):
        result = run("--service-account", self.account(kind="authorized_user"))
        self.assertEqual(result.returncode, 1)
        self.assertIn("service_account", result.stdout)

    def test_the_apk_check_detects_missing_and_embedded_identifiers(self):
        def apk(name: str, payload: bytes) -> str:
            path = self.dir / name
            with zipfile.ZipFile(path, "w") as z:
                z.writestr("classes.dex", payload)
            return str(path)
        missing = run("--apk", apk("senza.apk", b"dex\n035\x00 niente"))
        self.assertEqual(missing.returncode, 1)
        self.assertIn("NON contiene", missing.stdout)
        present = run("--apk", apk("con.apk", f"x {APP_ID} y {API_KEY} z".encode()))
        self.assertEqual(present.returncode, 0, present.stdout)
        self.assertNotIn(API_KEY, present.stdout)

    def test_without_arguments_it_prints_usage(self):
        self.assertEqual(run().returncode, 2)


if __name__ == "__main__":
    unittest.main()
