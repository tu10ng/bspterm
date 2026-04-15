#!/usr/bin/env python3
# [官方脚本] 此文件由 Bspterm 自动安装和更新，请勿修改。
# 如需自定义，请复制到新文件后再修改。
"""
Hidden Terminal 示例 / Hidden Terminal Example

演示如何创建后台隐藏终端，执行命令并读取输出，无需 UI 窗口。
Demonstrates creating a hidden background terminal, running commands
and reading output without a UI window.

@params
- host: string
  description: SSH 主机地址 / SSH host address
  required: true

- port: number
  description: SSH 端口 / SSH port
  default: 22

- username: string
  description: 用户名 / Username
  required: true

- password: password
  description: 密码 / Password
  required: true

- idle_timeout: number
  description: 空闲超时(秒), 0=不超时 / Idle timeout in seconds, 0=never
  default: 60
@end_params
"""

from bspterm import SSH, Session, toast, params


def main():
    host = params.host
    port = int(params.port or 22)
    username = params.username
    password = params.password
    idle_timeout = int(params.idle_timeout or 60)

    # 创建隐藏终端 / Create hidden terminal
    toast(f"正在连接 {host}:{port} ...")
    term = SSH.connect(
        host=host,
        port=port,
        user=username,
        password=password,
        hidden=True,
        idle_timeout=idle_timeout,
    )
    toast(f"已连接: {term.id}")

    # 执行命令 / Run commands
    output = term.run("hostname")
    toast(f"主机名: {output.output.strip()}")

    output = term.run("uptime")
    toast(f"运行时间: {output.output.strip()}")

    # 查看后台终端列表 / List hidden terminals
    hidden_list = Session.list_hidden()
    toast(f"后台终端数量: {len(hidden_list)}")

    # 手动关闭 / Close manually
    term.close()
    toast("隐藏终端已关闭")


if __name__ == "__main__":
    main()
