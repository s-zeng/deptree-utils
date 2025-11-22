"""Module with class method-level imports."""


class MyClass:
    def method(self):
        import base_module  # class method import
        return base_module.base_function()
