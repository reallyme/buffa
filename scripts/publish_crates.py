#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 ReallyMe Technologies Limited
# SPDX-License-Identifier: Apache-2.0
"""Publish checked archives in dependency order, resuming only identical uploads."""

import argparse
from http.client import HTTPException
import json
import os
from pathlib import Path
import subprocess
import sys
import time
import urllib.error
import urllib.request

from release_support import (DIGEST, ROOT, TRANSIENT_REGISTRY, Reason, Release,
                             ReleaseError, command, digest, reviewed_manifest, source)

ATTEMPTS = 12
RETRY_SECONDS = 30
MAX_RESPONSE_BYTES = 131_072


def registry_checksum(name: str, version: str) -> str | None:
    request = urllib.request.Request(
        "https://crates.io/api/v1/crates/" + name + "/" + version,
        headers={"User-Agent": "ReallyMe-Buffa-release (https://github.com/reallyme/buffa)"})
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            data = response.read(MAX_RESPONSE_BYTES + 1)
        if len(data) > MAX_RESPONSE_BYTES:
            raise ReleaseError(Reason.REGISTRY)
        package = json.loads(data)["version"]
        if (package["crate"] != name or package["num"] != version
                or package["yanked"] is not False or not isinstance(package["checksum"], str)
                or DIGEST.fullmatch(package["checksum"]) is None):
            raise ReleaseError(Reason.REGISTRY)
        return package["checksum"]
    except urllib.error.HTTPError as error:
        try:
            if error.code == 404:
                return None
            raise ReleaseError(Reason.REGISTRY) from error
        finally:
            error.close()
    except (OSError, HTTPException, ValueError, KeyError, TypeError) as error:
        raise ReleaseError(Reason.REGISTRY) from error


def registry_matches(name: str, version: str, checksum: str) -> bool:
    found = registry_checksum(name, version)
    if found is not None and found != checksum:
        raise ReleaseError(Reason.CHECKSUM)
    return found is not None


def publish_one(release: Release, name: str, checksum: str) -> None:
    # Repackage without running build scripts, then compare before uploading.
    # The predecessor job already built these exact archive bytes without a token.
    archive = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target")) / "package" / (
        name + "-" + release.version + ".crate")
    for attempt in range(ATTEMPTS):
        if source(publishing=True) != release:
            raise ReleaseError(Reason.SOURCE)
        try:
            command(["cargo", "package", "--registry", "crates-io", "--locked",
                     "--no-verify", "--all-features", "-p", name], registry_retry=True)
        except ReleaseError as error:
            # A published dependency can be visible in the API before the
            # sparse index. Retry packaging too, not just the upload command.
            if error.reason != Reason.REGISTRY_RETRY or attempt + 1 == ATTEMPTS:
                raise
            time.sleep(RETRY_SECONDS)
            continue
        if digest(archive) != checksum:
            raise ReleaseError(Reason.CHECKSUM)
        if registry_matches(name, release.version, checksum):
            print("Verified existing package: " + name, flush=True)
            return
        try:
            result = subprocess.run(
                ["cargo", "publish", "--registry", "crates-io", "--locked",
                 "--no-verify", "--all-features", "-p", name], cwd=ROOT,
                capture_output=True, text=True, timeout=900, check=False)
        except (OSError, subprocess.TimeoutExpired) as error:
            raise ReleaseError(Reason.PUBLISH) from error
        if result.returncode == 0:
            for poll in range(ATTEMPTS):
                if registry_matches(name, release.version, checksum):
                    print("Verified published package: " + name, flush=True)
                    return
                if poll + 1 < ATTEMPTS:
                    time.sleep(RETRY_SECONDS)
            raise ReleaseError(Reason.REGISTRY)
        # Cargo may have uploaded successfully before its index poll failed.
        if registry_matches(name, release.version, checksum):
            return
        if attempt + 1 == ATTEMPTS or TRANSIENT_REGISTRY.search(result.stdout + result.stderr) is None:
            raise ReleaseError(Reason.PUBLISH)
        print("Waiting for the registry before retrying: " + name, flush=True)
        time.sleep(RETRY_SECONDS)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--directory", required=True, type=Path)
    args = parser.parse_args()
    try:
        if os.environ.get("GITHUB_ACTIONS") != "true":
            raise ReleaseError(Reason.SOURCE)
        if not os.environ.get("CARGO_REGISTRY_TOKEN"):
            raise ReleaseError(Reason.CREDENTIAL)
        release = source(publishing=True)
        manifest = reviewed_manifest(release, args.directory)
        for package in manifest["packages"]:
            publish_one(release, package["name"], package["sha256"])
        return 0
    except ReleaseError as error:
        # Do not forward registry bodies or credentialed Cargo diagnostics.
        sys.stderr.write(error.reason.value + "\n")
        return 1


if __name__ == "__main__":
    sys.exit(main())
