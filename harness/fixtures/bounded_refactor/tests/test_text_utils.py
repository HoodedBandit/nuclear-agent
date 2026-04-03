import unittest

from text_utils import normalize_heading, normalize_title


class TextUtilsTests(unittest.TestCase):
    def test_normalize_title(self):
        self.assertEqual(normalize_title("  hello   world "), "Hello World")

    def test_normalize_heading(self):
        self.assertEqual(normalize_heading("  release   notes "), "RELEASE NOTES")


if __name__ == "__main__":
    unittest.main()

