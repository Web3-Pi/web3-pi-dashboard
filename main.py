"""
Author: Robert Mordzon
Organization: Web3Pi
Date: 2024-07-16
Description: This script checks if it is being run on a Raspberry Pi by examining system information and show CPu temperature.

License: GPL-3.0 license
Contact: robertmordzon@gmail.com
"""

import sys
import psutil

def main():
    if not hasattr(psutil, "sensors_temperatures"):
        sys.exit("platform not supported")
    temps = psutil.sensors_temperatures()
    if not temps:
        sys.exit("can't read any temperature")
    for name, entries in temps.items():
        print(name)
        for entry in entries:
            line = "    %-20s %s °C (high = %s °C, critical = %s °C)" % (
                entry.label or name,
                entry.current,
                entry.high,
                entry.critical,
            )
            print(line)
        print()


def is_raspberry_pi():
    """
    Checks if the script is running on a Raspberry Pi by examining the contents of /proc/cpuinfo.

    Returns:
        bool: True if running on Raspberry Pi, False otherwise.
    """
    try:
        with open('/proc/cpuinfo', 'r') as f:
            cpuinfo = f.read()
            if 'Raspberry Pi' in cpuinfo:
                return True
    except FileNotFoundError:
        return False
    return False


if __name__ == '__main__':
    if is_raspberry_pi():
        main()
    else:
        print("Only Raspberry Pi is supported")
        sys.exit("platform not supported")