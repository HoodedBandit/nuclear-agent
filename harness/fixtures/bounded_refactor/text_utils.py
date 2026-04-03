def normalize_title(text):
    text = text.strip()
    text = " ".join(text.split())
    return text.title()


def normalize_heading(text):
    text = text.strip()
    text = " ".join(text.split())
    return text.upper()

