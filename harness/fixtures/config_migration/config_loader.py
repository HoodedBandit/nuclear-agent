def load_api_url(data):
    if "api_url" not in data:
        raise KeyError("api_url")
    return data["api_url"].rstrip("/")

