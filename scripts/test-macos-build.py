#!/usr/bin/env python3
"""Check the macOS wrapper without downloading images or starting containers."""

import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


SOURCE = Path(__file__).resolve().parent / "build-macos.sh"


class MacOSBuildTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.repo = self.root / "checkout with spaces"
        (self.repo / "scripts").mkdir(parents=True)
        self.script = self.repo / "scripts" / SOURCE.name
        shutil.copy2(SOURCE, self.script)
        self.record = self.root / "arguments.json"
        self.bin = self.root / "bin"
        self.bin.mkdir()
        for engine in ("podman", "docker"):
            stub = self.bin / engine
            stub.write_text(
                "#!/usr/bin/env python3\n"
                "import json, os, sys\n"
                "with open(os.environ['ARGUMENT_RECORD'], 'w') as stream:\n"
                "    json.dump(sys.argv[1:], stream)\n"
                "sys.exit(int(os.environ.get('ENGINE_STATUS', '0')))\n"
            )
            stub.chmod(0o755)
        self.env = {
            **os.environ,
            "PATH": f"{self.bin}:{os.environ['PATH']}",
            "ARGUMENT_RECORD": str(self.record),
            "CONTAINER_ENGINE": "podman",
        }

    def run_script(self, *args):
        return subprocess.run(
            ["sh", str(self.script), *args],
            cwd=self.root,
            env=self.env,
            text=True,
            capture_output=True,
        )

    def test_build_from_elsewhere_with_spaces_and_pinned_environment(self):
        result = self.run_script()
        self.assertEqual(result.returncode, 0, result.stderr)
        args = json.loads(self.record.read_text())
        self.assertIn(f"{self.repo}:/io", args)
        self.assertIn("--userns=keep-id", args)
        self.assertIn(f"{os.getuid()}:{os.getgid()}", args)
        self.assertIn("MACOSX_DEPLOYMENT_TARGET=13.0", args)
        image = next(arg for arg in args if arg.startswith("ghcr.io/"))
        self.assertRegex(image, r"@sha256:[0-9a-f]{64}$")
        self.assertIn("--locked", args)
        self.assertIn("aarch64-apple-darwin", args)
        self.assertIn("x86_64-apple-darwin", args)
        self.assertTrue((self.repo / "target/apple").is_dir())

    def test_docker_uses_caller_without_podman_namespace_option(self):
        self.env["CONTAINER_ENGINE"] = "docker"
        self.assertEqual(self.run_script().returncode, 0)
        args = json.loads(self.record.read_text())
        self.assertIn("--user", args)
        self.assertNotIn("--userns=keep-id", args)

    def test_failed_build_does_not_report_success(self):
        self.env["ENGINE_STATUS"] = "42"
        result = self.run_script()
        self.assertEqual(result.returncode, 42)
        self.assertNotIn("Apple Silicon:", result.stdout)

    def test_invalid_engine_is_rejected_before_invocation(self):
        self.env["CONTAINER_ENGINE"] = "docker --privileged"
        self.assertEqual(self.run_script().returncode, 2)
        self.assertFalse(self.record.exists())

    def test_arguments_are_rejected_before_invocation(self):
        self.assertEqual(self.run_script("--publish").returncode, 2)
        self.assertFalse(self.record.exists())


if __name__ == "__main__":
    unittest.main()
