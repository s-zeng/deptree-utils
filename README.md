# deptree-utils

WIP

The end goal of this project is to:
- be able to derive and display the internal dependency tree of any project
- provide useful primitives for building on top of dependency tree tech

For instance, one of the main goals of this project is to calculate all the
files that would be affected by a change in a single file compared to a version
control base, so that that list of files could be passed on to e.g. a test
driver
