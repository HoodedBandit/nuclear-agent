import unittest

from calculator import add


class CalculatorTests(unittest.TestCase):
    def test_add_handles_positive_values(self):
        self.assertEqual(add(2, 3), 5)

    def test_add_handles_negative_values(self):
        self.assertEqual(add(-2, -5), -7)


if __name__ == "__main__":
    unittest.main()

