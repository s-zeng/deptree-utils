"""Another valid Python module that imports from valid_module."""

from valid_module import works

def also_works():
    """Uses the imported function."""
    result = works()
    return f"Result: {result}"
