"""Keep only the highest semantic-version GitHub release and its tag."""

import re
import subprocess
import sys

TAG = re.compile(
    r"^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)"
    r"(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?$"
)


def version_key(
    tag: str,
) -> tuple[tuple[int, int, int], int, tuple[tuple[int, int | str], ...]] | None:
    match = TAG.fullmatch(tag)
    if not match:
        return None

    core = (int(match.group(1)), int(match.group(2)), int(match.group(3)))
    prerelease = match.group(4)
    if prerelease is None:
        return core, 1, ()

    identifiers: list[tuple[int, int | str]] = []
    for identifier in prerelease.split("."):
        if identifier.isascii() and identifier.isdigit():
            if len(identifier) > 1 and identifier.startswith("0"):
                return None
            identifiers.append((0, int(identifier)))
        else:
            identifiers.append((1, identifier))
    return core, 0, tuple(identifiers)


def newest_tag(tags: list[str]) -> str:
    valid = [(key, tag) for tag in tags if (key := version_key(tag)) is not None]
    if not valid:
        raise ValueError("nessuna release con tag SemVer valido da mantenere")
    return max(valid, key=lambda item: item[0])[1]


def prune_releases(repository: str, run=subprocess.run) -> str:
    result = run(
        [
            "gh", "release", "list", "--repo", repository, "--limit", "1000",
            "--json", "tagName", "--jq", ".[].tagName",
        ],
        check=True,
        text=True,
        capture_output=True,
    )
    tags = [line.strip() for line in result.stdout.splitlines() if line.strip()]
    keep = newest_tag(tags)

    for tag in tags:
        if tag != keep:
            run(
                [
                    "gh", "release", "delete", tag, "--yes", "--cleanup-tag",
                    "--repo", repository,
                ],
                check=True,
            )
    return keep


if __name__ == "__main__":
    if len(sys.argv) != 2:
        raise SystemExit("uso: python3 scripts/prune_releases.py OWNER/REPOSITORY")
    print(f"release mantenuta: {prune_releases(sys.argv[1])}")
