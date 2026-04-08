import unittest

from formatter import slugify


class FormatterTests(unittest.TestCase):
    def test_slugify_removes_punctuation(self):
        self.assertEqual(slugify("Hello, World!"), "hello-world")

    def test_slugify_collapses_separator_runs(self):
        self.assertEqual(slugify("  Multiple --- Separators  "), "multiple-separators")


if __name__ == "__main__":
    unittest.main()

