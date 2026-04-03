import unittest

from config_loader import load_api_url


class ConfigLoaderTests(unittest.TestCase):
    def test_loads_current_api_url_key(self):
        self.assertEqual(load_api_url({"api_url": "https://api.example.com/"}), "https://api.example.com")

    def test_loads_legacy_base_url_key(self):
        self.assertEqual(load_api_url({"base_url": "https://legacy.example.com/"}), "https://legacy.example.com")


if __name__ == "__main__":
    unittest.main()

