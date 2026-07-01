import os
import subprocess
import tempfile
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
SANHEDRIN_HOOK = REPO_ROOT / "hooks" / "sanhedrin.sh"


class SanhedrinShellEnvTests(unittest.TestCase):
    def test_env_file_is_parsed_not_executed(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            marker = tmp_path / "executed"
            env_file = tmp_path / "vestige-sanhedrin.env"
            env_file.write_text(
                "\n".join(
                    [
                        "VESTIGE_SANHEDRIN_ENABLED='1'",
                        "VESTIGE_SANHEDRIN_PYTHON='python3'",
                        f"VESTIGE_SANHEDRIN_MODEL='$(touch {marker})'",
                        "UNKNOWN_KEY='$(touch should-not-run)'",
                    ]
                )
                + "\n",
                encoding="utf-8",
            )

            env = os.environ.copy()
            env["VESTIGE_SANHEDRIN_ENV"] = str(env_file)
            result = subprocess.run(
                ["bash", str(SANHEDRIN_HOOK)],
                input='{"transcript_path":"/does/not/exist"}',
                text=True,
                env=env,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                check=False,
            )

            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertFalse(marker.exists())


if __name__ == "__main__":
    unittest.main()
