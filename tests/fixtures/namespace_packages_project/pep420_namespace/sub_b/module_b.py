# Module B in PEP 420 namespace sub_b
# This imports through the namespace package
from pep420_namespace.sub_a.module_a import function_a

def function_b():
    """Function that uses module_a through namespace"""
    function_a()
