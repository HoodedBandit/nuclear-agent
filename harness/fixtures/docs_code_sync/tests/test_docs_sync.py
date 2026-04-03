import pathlib
import unittest

from release_info import VERSION


class DocsSyncTests(unittest.TestCase):
    def test_readme_mentions_current_version(self):
        readme = pathlib.Path("README.md").read_text(encoding="utf-8")
        self.assertIn(VERSION, readme)


if __name__ == "__main__":
    unittest.main()

