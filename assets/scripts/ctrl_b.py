#!/usr/bin/env python3
# [官方脚本] 此文件由 Bspterm 自动安装和更新，请勿修改。
# 如需自定义，请复制到新文件后再修改。
"""
自动发送 Ctrl+B 脚本 (Auto Send Ctrl+B Script)

监控终端输出，当检测到 "Press Ctrl+B to" 时自动发送 Ctrl+B 进入 BootROM 菜单。
支持设备反复重启时多次打断。脚本启动后持续运行，按 Ctrl+C 停止。

使用方法：
1. 在目标终端中启动此脚本
2. 脚本会持续监控终端输出
3. 每次检测到提示后自动发送 Ctrl+B
4. 按 Ctrl+C 停止监控
"""

import re
import time

from bspterm import current_terminal, toast


def main():
    term = current_terminal()
    pattern = re.compile(r"Press Ctrl\+B to")
    toast("Monitoring for 'Press Ctrl+B to'... (Ctrl+C to stop)", "info")

    count = 0
    with term.track() as tracker:
        # 先检查当前屏幕是否已有匹配（设备可能已经在等待）
        screen = term.read()
        if pattern.search(screen.text):
            term.send("\x02")
            count += 1
            toast(f"Ctrl+B sent! (#{count})", "success")

        # 持续监控新输出
        while True:
            new_output = tracker.read_new()
            if new_output and pattern.search(new_output):
                term.send("\x02")
                count += 1
                toast(f"Ctrl+B sent! (#{count})", "success")
            time.sleep(0.1)


if __name__ == "__main__":
    main()
