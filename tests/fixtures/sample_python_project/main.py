# Main entry point
from pkg_a import module_a
from pkg_b.module_b import helper

def main():
    module_a.do_something()
    helper()

if __name__ == "__main__":
    main()
