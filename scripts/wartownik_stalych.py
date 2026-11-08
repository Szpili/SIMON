#!/usr/bin/env python3
"""Compatibility wrapper for scripts/constant_guard.py."""
import pathlib
import runpy
import sys

guard = pathlib.Path(__file__).parent / "constant_guard.py"
runpy.run_path(str(guard), run_name="__main__")
