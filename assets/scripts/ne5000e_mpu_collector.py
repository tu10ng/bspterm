#!/usr/bin/env python3
"""
NE5000E MPU IP Collector Script

This script collects IP addresses from all MPU boards in a Huawei NE5000E router
and adds them as SSH sessions to the current session group.

Usage:
    1. Connect to the NE5000E router via SSH/Telnet
    2. Run this script from the Script Panel

The script will:
    1. Split the current terminal to the right
    2. Execute commands in the new terminal to collect MPU IPs
    3. Add SSH sessions for each MPU to the session group
    4. Show toast notifications for success/failure
"""

import re
from bspterm import current_terminal, Pane, Session, toast

SSH_USERNAME = "root"
SSH_PASSWORD = "root"


def main():
    term = current_terminal()
    group_info = Session.get_current_group(term.id)
    group_id = group_info.get("group_id")

    if not group_id:
        toast("Current terminal does not belong to any session group", "error")
        return

    right_term = Pane.split_right_clone(term.id, wait_for_login=True)

    right_term.sendcmd("screen-length 0 temporary")

    output = right_term.sendcmd("display device")
    mpus = parse_mpu_slots(output)

    if not mpus:
        toast("No MPU boards found", "warning")
        return

    right_term.sendcmd("sys", prompt_pattern=r"\[.*\]")
    right_term.sendcmd("diagnose", prompt_pattern=r"\[.*-diagnose\]")

    failed_slots = []
    success_count = 0

    for slot_id in mpus:
        try:
            ip = get_mpu_ip(right_term, slot_id)
            if ip:
                session_name = f"Slot{slot_id}-{ip}"
                Session.add_ssh_to_group(
                    group_id=group_id,
                    name=session_name,
                    host=ip,
                    port=22,
                    username=SSH_USERNAME,
                    password=SSH_PASSWORD,
                )
                success_count += 1
            else:
                failed_slots.append(slot_id)
        except Exception:
            failed_slots.append(slot_id)

    right_term.sendcmd("return")

    if failed_slots:
        toast(f"Failed slots: {', '.join(map(str, failed_slots))}", "error")

    if success_count > 0:
        toast(f"Added {success_count} SSH sessions", "success")


def parse_mpu_slots(output: str) -> list:
    """
    Parse display device output to extract MPU slot identifiers.

    NE5000E slot format can be:
    - Pure number: 21
    - Text format: clc1/21

    Example lines:
    21    -    MPU               Present   PowerOn  Registered   Normal   Master
    clc1/21  1  CR5DMPUA2Y2       Present   PowerOn  Registered   Normal   NA
    """
    mpus = []
    pattern = r"^(\S+)\s+.*\bMPU"
    for line in output.split("\n"):
        line = line.strip()
        if not line or line.startswith("-"):
            continue
        match = re.match(pattern, line, re.IGNORECASE)
        if match:
            slot_id = match.group(1)
            mpus.append(slot_id)
    return mpus


def get_mpu_ip(term, slot_id: str) -> str:
    """
    Get the eth0/eth1 IP address for a slot.

    Args:
        term: Terminal instance
        slot_id: Slot identifier (e.g., "21" or "clc1/21")

    Returns:
        IP address string or None if not found

    Note:
        Uses prompt_pattern=r"\\[.*-diagnose\\]" to match VRP diagnose mode
        prompt (e.g., [Huawei-diagnose]) instead of the default [$#>] pattern.
    """
    output = term.sendcmd(
        f'shell slot {slot_id} "ifconfig"',
        timeout=10,
        prompt_pattern=r"\[.*-diagnose\]"
    )

    patterns = [
        r"eth[01].*?inet addr:(\d+\.\d+\.\d+\.\d+)",
        r"eth[01].*?inet\s+(\d+\.\d+\.\d+\.\d+)",
    ]
    for pattern in patterns:
        match = re.search(pattern, output, re.DOTALL | re.IGNORECASE)
        if match:
            return match.group(1)
    return None


if __name__ == "__main__":
    main()
