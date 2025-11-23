# Consumer module in normal package
# Imports from both namespace packages
from pep420_namespace.sub_b.module_b import function_b
from legacy_namespace.submodule.module import legacy_function

def use_namespaces():
    """Function that uses both namespace packages"""
    function_b()
    legacy_function()
