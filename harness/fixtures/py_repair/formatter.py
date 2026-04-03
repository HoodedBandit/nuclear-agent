import re


def slugify(text):
    text = text.strip().lower()
    text = re.sub(r"\s+", "-", text)
    return text

