"""Module with function-level imports."""

import base_module  # top-level import


def my_function():
    import another_module  # function-level import
    return another_module.another_function()
