# Module A - depends on pkg_b
from pkg_b import module_b

def do_something():
    return module_b.helper()
