"""Runner script that imports both internal module and other script"""
from foo.bar import some_function
from .utils.helper import helper_function

def run():
    some_function()
    helper_function()
