#!/usr/bin/env python3
"""
设备上线通知脚本 (Device Online Notification Script)

当断连的设备重新上线时，此脚本被调用。
可以自定义通知方式：邮件、Slack、微信等。

环境变量：
- BSPTERM_SOCKET: 脚本服务器连接字符串
- BSPTERM_RECONNECTED_TERMINALS: JSON 数组，包含上线的终端信息

JSON 格式示例：
[
  {"terminal_id": "123", "host": "192.168.1.1", "group_id": "abc", "group_name": "Router"},
  {"terminal_id": "456", "host": "192.168.1.2", "group_id": "abc", "group_name": "Router"}
]
"""
import json
import os
import sys


def main():
    # 获取上线的终端信息
    terminals_json = os.environ.get("BSPTERM_RECONNECTED_TERMINALS", "[]")

    try:
        terminals = json.loads(terminals_json)
    except json.JSONDecodeError:
        print("Error: Invalid JSON in BSPTERM_RECONNECTED_TERMINALS", file=sys.stderr)
        return 1

    if not terminals:
        print("No terminals reconnected")
        return 0

    # 输出上线信息
    print(f"=== {len(terminals)} 台设备上线 ===")
    for t in terminals:
        host = t.get("host", "unknown")
        group_name = t.get("group_name", "")
        if group_name:
            print(f"  - {host} ({group_name})")
        else:
            print(f"  - {host}")

    # TODO: 在此添加自定义通知逻辑
    # 例如：发送邮件、Slack 消息、微信通知等
    #
    # 示例：发送到 webhook
    # import urllib.request
    # webhook_url = "https://your-webhook-url"
    # data = json.dumps({"text": f"{len(terminals)} devices online"}).encode()
    # req = urllib.request.Request(webhook_url, data=data, headers={"Content-Type": "application/json"})
    # urllib.request.urlopen(req)

    return 0


if __name__ == "__main__":
    sys.exit(main())
