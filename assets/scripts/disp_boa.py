#!/usr/bin/env python3
"""
Display BOA Information Script

This script sends the 'disp boa {slotid}' command to query BOA (Board On-line Aging)
information for a specific slot on Huawei devices.

Can be invoked in two ways:
1. From Script Panel with @params dialog:
   - Click the script and fill in slotid

2. From Function Bar (type in terminal and press Enter):
   - disp_boa 21

@params
- slotid: string
  description: Slot ID (e.g., 1, 2, 1/0)
  required: true
@end_params
"""

from bspterm import current_terminal, params, args


def main():
    term = current_terminal()

    # First try to get slotid from function args (invoked from terminal)
    # Then fall back to params (invoked from script panel)
    if len(args) > 0:
        slotid = args[0]
    elif hasattr(params, 'slotid'):
        slotid = params.slotid
    else:
        slotid = "0"

    term.send(f"disp boa {slotid}\n")


if __name__ == "__main__":
    main()
