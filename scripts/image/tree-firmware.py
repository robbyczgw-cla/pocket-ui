#!/usr/bin/env python3
"""Firmware paths from dtc output, without neighbouring sound labels."""
import re
import sys


def firmware_names(dts):
    assignments = re.findall(r'(?m)^[ \t]*firmware-name[ \t]*=[ \t]*(.*?);', dts, re.S)
    return sorted({name for value in assignments for quoted in re.findall(r'"([^"]*)"', value)
                   for name in quoted.split(r'\0') if name})


if __name__ == '__main__':
    print('\n'.join(firmware_names(sys.stdin.read())))
