"""Il repository non deve contenere dati sensibili (ADR-0236): chiavi, token, chiavi private, indirizzi personali.

Il controllo guarda solo i file tracciati da git. Le fixture di test usano valori sintetici su domini riservati
(example.com, .invalid, .test) e segnaposto evidenti.
"""
import re
import subprocess
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]

SECRET_PATTERNS = {
    "chiave NVIDIA": re.compile(r"nvapi-[A-Za-z0-9_\-]{20,}"),
    "token Hugging Face": re.compile(r"\bhf_[A-Za-z0-9]{30,}\b"),
    "token GitHub": re.compile(r"\b(?:ghp|gho|ghu|ghs|ghr)_[A-Za-z0-9]{30,}\b|github_pat_[A-Za-z0-9_]{30,}"),
    "chiave AWS": re.compile(r"\bAKIA[0-9A-Z]{16}\b"),
    # Una chiave privata vera ha un corpo lungo; l'intestazione sola (nel codice che la legge) non conta.
    "chiave privata": re.compile(r"-----BEGIN (?:RSA |EC |DSA |OPENSSH |PGP )?PRIVATE KEY-----\s*[A-Za-z0-9+/=\s]{200,}"),
    "chiave API generica": re.compile(r"\bsk-[A-Za-z0-9]{32,}\b"),
    "token Slack": re.compile(r"\bxox[baprs]-[A-Za-z0-9-]{10,}"),
    "chiave Google": re.compile(r"\bAIza[0-9A-Za-z_\-]{35}\b"),
    "segreto client Google": re.compile(r"GOCSPX-[A-Za-z0-9_\-]{20,}"),
}

# Segnaposto evidenti delle fixture di test: sequenze dell'alfabeto o di cifre, «xxxx», «fixture»…
PLACEHOLDER = re.compile(r"abcdefghijk|0123456789|12345|xxxx|fixture|synthetic|sintetic|example|dummy|placeholder", re.I)

EMAIL = re.compile(r"\b[A-Za-z0-9._%+\-]+@([A-Za-z0-9.\-]+\.[A-Za-z]{2,})\b")
# Gli indirizzi nelle fixture sono sintetici (esempio.it, x.it…). Il rischio vero è un indirizzo personale su un
# fornitore pubblico: quei domini non devono comparire mai, nemmeno nei test (usare example.com o .invalid).
PUBLIC_MAIL_DOMAINS = {
    "gmail.com", "googlemail.com", "outlook.com", "outlook.it", "hotmail.com", "hotmail.it", "live.com", "live.it",
    "icloud.com", "me.com", "mac.com", "yahoo.com", "yahoo.it", "ymail.com", "proton.me", "protonmail.com",
    "libero.it", "virgilio.it", "tiscali.it", "alice.it", "fastwebnet.it", "aruba.it", "pec.it", "gmx.com", "gmx.net",
    "aol.com", "msn.com", "mail.com", "yandex.com", "zoho.com",
}
TEXT_SUFFIXES = {
    ".rs", ".py", ".swift", ".kt", ".kts", ".md", ".json", ".toml", ".yml", ".yaml", ".sh", ".ps1", ".cjs",
    ".js", ".ts", ".html", ".css", ".txt", ".tsv", ".xml", ".plist", ".gradle", ".properties", ".cmd", ".nsi",
}
SKIP_NAMES = {"Cargo.lock", "package-lock.json", "requirements.lock", "SHA256SUMS"}


def tracked_text_files():
    out = subprocess.run(["git", "ls-files", "-z"], cwd=ROOT, capture_output=True, check=True).stdout
    for raw in out.split(b"\0"):
        if not raw:
            continue
        path = ROOT / raw.decode()
        if path.suffix.lower() in TEXT_SUFFIXES and path.name not in SKIP_NAMES and path.is_file():
            yield path


class NoSensitiveData(unittest.TestCase):
    def test_no_secrets_in_tracked_files(self):
        found = []
        for path in tracked_text_files():
            text = path.read_text(encoding="utf-8", errors="ignore")
            for label, pattern in SECRET_PATTERNS.items():
                for match in pattern.finditer(text):
                    if label != "chiave privata" and PLACEHOLDER.search(match.group(0)):
                        continue
                    found.append(f"{path.relative_to(ROOT)}: {label}")
                    break
        self.assertEqual(found, [], "segreti nel repository: " + "; ".join(found))

    def test_no_real_email_addresses_in_tracked_files(self):
        found = []
        for path in tracked_text_files():
            text = path.read_text(encoding="utf-8", errors="ignore")
            for match in EMAIL.finditer(text):
                if match.group(1).lower() in PUBLIC_MAIL_DOMAINS:
                    # Non si stampa l'indirizzo: basta dire dove si trova.
                    found.append(f"{path.relative_to(ROOT)}: indirizzo su un fornitore pubblico")
        self.assertEqual(found, [], "indirizzi personali nel repository: " + "; ".join(sorted(set(found))))

    def test_environment_and_key_files_are_ignored(self):
        ignored = (ROOT / ".gitignore").read_text()
        for needle in (".env", "*.key", "*.pem", "data/"):
            self.assertIn(needle, ignored, f"{needle} deve essere ignorato da git")
        tracked = subprocess.run(["git", "ls-files"], cwd=ROOT, capture_output=True, text=True, check=True).stdout.splitlines()
        risky = [f for f in tracked if f.endswith((".env", ".key", ".pem", ".p12", ".pfx", ".sqlite3", ".sqlite"))]
        self.assertEqual(risky, [], f"file sensibili tracciati: {risky}")


if __name__ == "__main__":
    unittest.main()
