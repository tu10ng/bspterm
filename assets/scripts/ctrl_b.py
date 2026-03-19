#!/usr/bin/env python3
# [官方脚本] 此文件由 Bspterm 自动安装和更新，请勿修改。
# 如需自定义，请复制到新文件后再修改。
"""
自动发送 Ctrl+B 脚本 (Auto Send Ctrl+B Script)

监控终端输出，当检测到 "Press Ctrl+B to" 时自动发送 Ctrl+B 进入 BootROM 菜单。
脚本启动后绑定到当前终端，即使切换 tab 也会持续监控。

使用方法：
1. 在目标终端中启动此脚本
2. 脚本会等待最多 300 秒
3. 检测到提示后自动发送 Ctrl+B
"""

from bspterm import current_terminal, toast


def main():
    term = current_terminal()
    toast("Waiting for 'Press Ctrl+B to'...", "info")
    term.wait_for(r"Press Ctrl\+B to", timeout=300)
    term.send("\x02")
    toast("Ctrl+B sent!", "success")


if __name__ == "__main__":
    main()
