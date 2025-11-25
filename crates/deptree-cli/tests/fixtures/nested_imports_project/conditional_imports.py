"""Module with conditional imports."""


def check_condition():
    if True:
        import base_module  # import in if block
        return base_module.base_function()


def handle_errors():
    try:
        import another_module  # import in try block
        return another_module.another_function()
    except ImportError:
        return None
