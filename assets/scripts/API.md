# BSPTerm Python Scripting API Reference

BSPTerm 是一款专业的终端模拟器，支持 SSH、Telnet 等多种连接协议。本文档详细介绍了 BSPTerm 提供的 Python 脚本 API，用于自动化终端操作。

## 特性

- **多协议支持**: SSH、Telnet 连接自动化
- **纯净输出**: `sendcmd()` 自动去除命令回显和提示符
- **增量跟踪**: 精确捕获命令输出
- **批量操作**: 支持批量设备管理和巡检
- **网络设备兼容**: 支持华为、思科等网络设备的自定义提示符

## 目录

- [快速开始](#快速开始)
- [核心函数](#核心函数)
  - [current_terminal()](#current_terminal)
  - [Session.list()](#sessionlist)
  - [Session.get()](#sessionget)
- [Terminal 类](#terminal-类)
  - [属性](#属性)
  - [send()](#send)
  - [sendcmd()](#sendcmd)
  - [run()](#run)
  - [read() / screen()](#read--screen)
  - [wait_for()](#wait_for)
  - [track()](#track)
  - [run_marked()](#run_marked)
  - [read_time_range()](#read_time_range)
  - [close()](#close)
- [TrackingSession 类](#trackingsession-类)
- [SSH 连接](#ssh-连接)
- [Telnet 连接](#telnet-连接)
- [数据类](#数据类)
- [异常处理](#异常处理)
- [完整示例](#完整示例)

---

## 快速开始

```python
#!/usr/bin/env python3
"""快速开始示例 - 保存为 ~/.config/bspterm/scripts/quickstart.py"""

from bspterm import current_terminal

# 获取当前终端
term = current_terminal()

# 执行命令并获取纯净输出
result = term.sendcmd("echo 'Hello, BSPTerm!'")
print(f"输出: {result}")

# 执行多个命令
hostname = term.sendcmd("hostname")
pwd = term.sendcmd("pwd")
print(f"主机名: {hostname}")
print(f"当前目录: {pwd}")
```

---

## 核心函数

### current_terminal()

获取当前聚焦的终端实例。当从 Script Panel 运行脚本时，返回启动脚本时聚焦的终端。

**签名:**
```python
def current_terminal() -> Terminal
```

**返回值:**
- `Terminal` - 当前终端的实例

**异常:**
- `TerminalNotFoundError` - 没有聚焦的终端

**示例:**
```python
#!/usr/bin/env python3
"""current_terminal 示例 - 保存为 ~/.config/bspterm/scripts/example_current.py"""

from bspterm import current_terminal, TerminalNotFoundError

try:
    term = current_terminal()
    print(f"终端 ID: {term.id}")
    print(f"终端类型: {term.type}")
    print(f"连接状态: {'已连接' if term.connected else '已断开'}")
except TerminalNotFoundError:
    print("错误: 请先聚焦一个终端窗口")
```

---

### Session.list()

列出所有可用的终端会话。

**签名:**
```python
@staticmethod
def list() -> List[SessionInfo]
```

**返回值:**
- `List[SessionInfo]` - 所有终端会话的信息列表

**示例:**
```python
#!/usr/bin/env python3
"""列出所有会话 - 保存为 ~/.config/bspterm/scripts/example_list.py"""

from bspterm import Session

sessions = Session.list()

if not sessions:
    print("没有打开的终端会话")
else:
    print(f"共有 {len(sessions)} 个终端会话:\n")
    for s in sessions:
        status = "已连接" if s.connected else "已断开"
        print(f"  ID: {s.id}")
        print(f"  名称: {s.name}")
        print(f"  类型: {s.type}")
        print(f"  状态: {status}")
        print()
```

---

### Session.get()

通过 ID 获取指定的终端实例。

**签名:**
```python
@staticmethod
def get(terminal_id: str) -> Terminal
```

**参数:**
- `terminal_id` (str) - 终端的唯一标识符

**返回值:**
- `Terminal` - 指定终端的实例

**异常:**
- `TerminalNotFoundError` - 指定 ID 的终端不存在

**示例:**
```python
#!/usr/bin/env python3
"""通过 ID 获取终端 - 保存为 ~/.config/bspterm/scripts/example_get.py"""

from bspterm import Session, TerminalNotFoundError

# 首先列出所有会话获取 ID
sessions = Session.list()

if sessions:
    # 获取第一个会话
    first_session = sessions[0]
    print(f"获取终端: {first_session.id}")

    # 通过 ID 获取 Terminal 实例
    term = Session.get(first_session.id)

    # 或者直接使用 SessionInfo.connect()
    term2 = first_session.connect()

    # 现在可以操作终端
    result = term.sendcmd("whoami")
    print(f"当前用户: {result}")
else:
    print("没有可用的终端")
```

---

## Terminal 类

Terminal 类是终端自动化的核心，提供了发送命令、读取输出等功能。

### 属性

| 属性 | 类型 | 描述 |
|------|------|------|
| `id` | str | 终端的唯一标识符 |
| `connected` | bool | 终端是否已连接 |
| `type` | str | 终端类型 ("local", "ssh", "telnet") |

**示例:**
```python
#!/usr/bin/env python3
"""Terminal 属性示例 - 保存为 ~/.config/bspterm/scripts/example_attrs.py"""

from bspterm import current_terminal

term = current_terminal()

print(f"终端 ID: {term.id}")
print(f"连接状态: {term.connected}")
print(f"终端类型: {term.type}")
```

---

### send()

向终端发送原始输入数据。这是最底层的输入方法，不会等待响应。

**签名:**
```python
def send(self, data: str) -> None
```

**参数:**
- `data` (str) - 要发送的原始数据，包括换行符

**示例:**
```python
#!/usr/bin/env python3
"""send() 示例 - 保存为 ~/.config/bspterm/scripts/example_send.py"""

import time
from bspterm import current_terminal

term = current_terminal()

# 发送命令（需要手动添加换行符）
term.send("echo 'Hello World'\n")

# 等待命令执行
time.sleep(0.5)

# 读取屏幕内容查看结果
screen = term.read()
print(f"屏幕内容:\n{screen.text}")
```

**交互式命令示例:**
```python
#!/usr/bin/env python3
"""交互式命令示例 - 保存为 ~/.config/bspterm/scripts/example_interactive.py"""

import time
from bspterm import current_terminal

term = current_terminal()

# 模拟交互式登录
term.send("ssh user@192.168.1.1\n")
time.sleep(2)

# 等待密码提示
term.wait_for("password:")

# 发送密码
term.send("mypassword\n")
time.sleep(1)

# 等待登录成功
term.wait_for(r"[$#>]")
print("登录成功!")
```

---

### sendcmd()

执行命令并返回**纯净的命令输出**（自动去除命令回显和提示符）。这是最推荐的执行命令方式。

**签名:**
```python
def sendcmd(
    self,
    command: str,
    timeout: float = 30.0,
    prompt_pattern: str = None
) -> str
```

**参数:**
- `command` (str) - 要执行的命令
- `timeout` (float) - 超时时间（秒），默认 30 秒
- `prompt_pattern` (str) - 检测命令完成的正则表达式，默认 `[$#>]\s*$`

**返回值:**
- `str` - 命令的纯净输出（不含命令回显和提示符）

**异常:**
- `TimeoutError` - 命令在超时时间内未完成

**示例:**
```python
#!/usr/bin/env python3
"""sendcmd() 基础示例 - 保存为 ~/.config/bspterm/scripts/example_sendcmd.py"""

from bspterm import current_terminal

term = current_terminal()

# 执行命令获取纯净输出
result = term.sendcmd("ls -la")
print("文件列表:")
print(result)

# 输出不包含 "ls -la" 命令本身，也不包含提示符
```

**获取系统信息示例:**
```python
#!/usr/bin/env python3
"""获取系统信息 - 保存为 ~/.config/bspterm/scripts/example_sysinfo.py"""

from bspterm import current_terminal

term = current_terminal()

# 获取各种系统信息
hostname = term.sendcmd("hostname").strip()
kernel = term.sendcmd("uname -r").strip()
uptime = term.sendcmd("uptime").strip()
mem_info = term.sendcmd("free -h | head -2")
disk_info = term.sendcmd("df -h /")

print("=" * 50)
print("系统信息报告")
print("=" * 50)
print(f"主机名: {hostname}")
print(f"内核版本: {kernel}")
print(f"运行时间: {uptime}")
print(f"\n内存使用:\n{mem_info}")
print(f"\n磁盘使用:\n{disk_info}")
```

**自定义提示符示例:**
```python
#!/usr/bin/env python3
"""自定义提示符 - 保存为 ~/.config/bspterm/scripts/example_custom_prompt.py"""

from bspterm import current_terminal

term = current_terminal()

# 网络设备通常使用不同的提示符
# 华为设备: <hostname> 或 [hostname]
# 思科设备: hostname# 或 hostname>

# 华为设备示例
output = term.sendcmd(
    "display version",
    prompt_pattern=r"[<\[].*[>\]]$"
)
print(output)

# 思科设备示例
output = term.sendcmd(
    "show version",
    prompt_pattern=r"\S+[#>]$"
)
print(output)
```

**批量命令执行示例:**
```python
#!/usr/bin/env python3
"""批量执行命令 - 保存为 ~/.config/bspterm/scripts/example_batch.py"""

from bspterm import current_terminal

term = current_terminal()

commands = [
    "date",
    "whoami",
    "pwd",
    "echo $SHELL",
]

results = {}
for cmd in commands:
    results[cmd] = term.sendcmd(cmd).strip()

print("命令执行结果:")
for cmd, output in results.items():
    print(f"  {cmd}: {output}")
```

---

### run()

执行命令并等待完成。与 `sendcmd()` 类似，但输出可能包含命令回显。

**签名:**
```python
def run(
    self,
    command: str,
    timeout: float = 30.0,
    prompt_pattern: str = None
) -> str
```

**参数:**
- `command` (str) - 要执行的命令
- `timeout` (float) - 超时时间（秒），默认 30 秒
- `prompt_pattern` (str) - 检测命令完成的正则表达式

**返回值:**
- `str` - 命令输出（可能包含命令回显）

**异常:**
- `TimeoutError` - 命令在超时时间内未完成

**示例:**
```python
#!/usr/bin/env python3
"""run() 示例 - 保存为 ~/.config/bspterm/scripts/example_run.py"""

from bspterm import current_terminal

term = current_terminal()

# run() 返回的输出可能包含命令本身
output = term.run("echo hello")
print(f"输出: {repr(output)}")

# 对于需要纯净输出的场景，推荐使用 sendcmd()
clean_output = term.sendcmd("echo hello")
print(f"纯净输出: {repr(clean_output)}")
```

---

### read() / screen()

读取当前终端屏幕内容。`screen()` 是 `read()` 的别名。

**签名:**
```python
def read(self) -> Screen
def screen(self) -> Screen  # 别名
```

**返回值:**
- `Screen` - 包含屏幕内容和光标位置的对象

**Screen 对象属性:**
| 属性 | 类型 | 描述 |
|------|------|------|
| `text` | str | 屏幕上的完整文本内容 |
| `cursor_row` | int | 光标所在行（从 0 开始） |
| `cursor_col` | int | 光标所在列（从 0 开始） |
| `rows` | int | 终端行数 |
| `cols` | int | 终端列数 |

**示例:**
```python
#!/usr/bin/env python3
"""read() 示例 - 保存为 ~/.config/bspterm/scripts/example_read.py"""

from bspterm import current_terminal

term = current_terminal()

# 读取屏幕内容
screen = term.read()

print(f"终端大小: {screen.rows} 行 x {screen.cols} 列")
print(f"光标位置: 行 {screen.cursor_row}, 列 {screen.cursor_col}")
print(f"\n屏幕内容:\n{'-' * 40}")
print(screen.text)
print('-' * 40)
```

**屏幕内容分析示例:**
```python
#!/usr/bin/env python3
"""分析屏幕内容 - 保存为 ~/.config/bspterm/scripts/example_analyze.py"""

from bspterm import current_terminal

term = current_terminal()

# 执行命令
term.send("ls -la\n")

import time
time.sleep(0.5)

# 读取并分析屏幕
screen = term.read()
lines = screen.text.split('\n')

print(f"屏幕共 {len(lines)} 行")
print(f"最后一行: {repr(lines[-1])}")

# 检查是否有特定内容
if "total" in screen.text:
    print("检测到 ls 输出")
```

---

### wait_for()

等待终端输出中出现匹配指定正则表达式的内容。

**签名:**
```python
def wait_for(
    self,
    pattern: str,
    timeout: float = 30.0
) -> str
```

**参数:**
- `pattern` (str) - 要匹配的正则表达式
- `timeout` (float) - 超时时间（秒），默认 30 秒

**返回值:**
- `str` - 匹配时的屏幕内容

**异常:**
- `TimeoutError` - 在超时时间内未匹配到模式

**示例:**
```python
#!/usr/bin/env python3
"""wait_for() 示例 - 保存为 ~/.config/bspterm/scripts/example_wait.py"""

from bspterm import current_terminal, TimeoutError

term = current_terminal()

# 等待提示符出现
try:
    content = term.wait_for(r"[$#>]\s*$", timeout=5)
    print("检测到 shell 提示符")
except TimeoutError:
    print("等待提示符超时")
```

**SSH 登录示例:**
```python
#!/usr/bin/env python3
"""SSH 登录流程 - 保存为 ~/.config/bspterm/scripts/example_ssh_login.py"""

from bspterm import current_terminal, TimeoutError

term = current_terminal()

host = "192.168.1.100"
username = "admin"
password = "secret"

try:
    # 发起 SSH 连接
    term.send(f"ssh {username}@{host}\n")

    # 等待密码提示或已知主机确认
    content = term.wait_for(r"(password:|yes/no)", timeout=10)

    if "yes/no" in content:
        # 首次连接，确认主机指纹
        term.send("yes\n")
        term.wait_for("password:", timeout=5)

    # 输入密码
    term.send(f"{password}\n")

    # 等待登录成功（检测到提示符）
    term.wait_for(r"[$#>]", timeout=10)

    print(f"成功登录到 {host}")

    # 执行命令
    result = term.sendcmd("hostname")
    print(f"远程主机名: {result.strip()}")

except TimeoutError as e:
    print(f"登录失败: {e}")
```

---

### track()

开始增量输出跟踪。返回一个 `TrackingSession` 对象，用于只读取自上次读取以来的新输出。

**签名:**
```python
def track(self) -> TrackingSession
```

**返回值:**
- `TrackingSession` - 跟踪会话对象

**示例:**
```python
#!/usr/bin/env python3
"""track() 示例 - 保存为 ~/.config/bspterm/scripts/example_track.py"""

import time
from bspterm import current_terminal

term = current_terminal()

# 方式 1: 手动管理
tracker = term.track()

term.send("echo 'First command'\n")
time.sleep(0.5)
output1 = tracker.read_new()
print(f"第一个命令输出: {output1}")

term.send("echo 'Second command'\n")
time.sleep(0.5)
output2 = tracker.read_new()
print(f"第二个命令输出: {output2}")

tracker.stop()

# 方式 2: 使用上下文管理器（推荐）
with term.track() as tracker:
    term.send("date\n")
    time.sleep(0.5)
    output = tracker.read_new()
    print(f"日期: {output}")
```

**监控长时间任务示例:**
```python
#!/usr/bin/env python3
"""监控长时间任务 - 保存为 ~/.config/bspterm/scripts/example_monitor.py"""

import time
from bspterm import current_terminal

term = current_terminal()

print("开始监控编译任务...")

with term.track() as tracker:
    # 启动一个模拟的长时间任务
    term.send("for i in 1 2 3 4 5; do echo \"Step $i\"; sleep 1; done\n")

    # 持续读取新输出
    for _ in range(10):
        time.sleep(0.5)
        new_output = tracker.read_new()
        if new_output.strip():
            print(f"[新输出] {new_output.strip()}")

print("监控结束")
```

---

### run_marked()

执行命令并精确捕获其输出，使用内部标记跟踪命令边界。

**签名:**
```python
def run_marked(
    self,
    command: str,
    timeout: float = 30.0,
    prompt_pattern: str = None
) -> CommandResult
```

**参数:**
- `command` (str) - 要执行的命令
- `timeout` (float) - 超时时间（秒），默认 30 秒
- `prompt_pattern` (str) - 检测命令完成的正则表达式

**返回值:**
- `CommandResult` - 包含命令 ID、输出和退出码的对象

**CommandResult 属性:**
| 属性 | 类型 | 描述 |
|------|------|------|
| `command_id` | str | 命令的唯一标识符 |
| `output` | str | 命令输出 |
| `exit_code` | Optional[int] | 退出码（如果可用） |

**示例:**
```python
#!/usr/bin/env python3
"""run_marked() 示例 - 保存为 ~/.config/bspterm/scripts/example_marked.py"""

from bspterm import current_terminal

term = current_terminal()

# 执行命令并获取详细结果
result = term.run_marked("ls -la /tmp")

print(f"命令 ID: {result.command_id}")
print(f"输出:\n{result.output}")
print(f"退出码: {result.exit_code}")
```

---

### read_time_range()

读取指定时间范围内的输出。时间从终端开始跟踪时计算。

**签名:**
```python
def read_time_range(
    self,
    start_seconds: float,
    end_seconds: float
) -> str
```

**参数:**
- `start_seconds` (float) - 起始时间（秒）
- `end_seconds` (float) - 结束时间（秒）

**返回值:**
- `str` - 指定时间范围内的输出

**示例:**
```python
#!/usr/bin/env python3
"""read_time_range() 示例 - 保存为 ~/.config/bspterm/scripts/example_timerange.py"""

import time
from bspterm import current_terminal

term = current_terminal()

# 先开始跟踪
with term.track() as tracker:
    term.send("echo 'Time 0'\n")
    time.sleep(1)

    term.send("echo 'Time 1'\n")
    time.sleep(1)

    term.send("echo 'Time 2'\n")
    time.sleep(1)

# 读取 0.5 到 1.5 秒之间的输出
output = term.read_time_range(0.5, 1.5)
print(f"0.5-1.5 秒的输出:\n{output}")
```

---

### close()

关闭终端连接。主要用于关闭后台创建的 SSH/Telnet 连接。

**签名:**
```python
def close(self) -> None
```

**示例:**
```python
#!/usr/bin/env python3
"""close() 示例 - 保存为 ~/.config/bspterm/scripts/example_close.py"""

from bspterm import SSH

# 创建后台 SSH 连接
term = SSH.connect(
    host="192.168.1.100",
    user="admin",
    password="secret"
)

try:
    # 执行操作
    term.wait_for(r"[$#>]", timeout=10)
    result = term.sendcmd("hostname")
    print(f"主机名: {result}")
finally:
    # 完成后关闭连接
    term.close()
    print("连接已关闭")
```

---

## TrackingSession 类

用于增量跟踪终端输出的会话类。

### 属性

| 属性 | 类型 | 描述 |
|------|------|------|
| `terminal_id` | str | 关联的终端 ID |
| `reader_id` | str | 跟踪会话的唯一 ID |

### 方法

#### read_new()

读取自上次调用以来的新输出。

**签名:**
```python
def read_new(self) -> str
```

**返回值:**
- `str` - 新输出内容，如果没有新输出则返回空字符串

**异常:**
- `BsptermError` - 如果跟踪会话已停止

#### stop()

停止跟踪会话并释放资源。

**签名:**
```python
def stop(self) -> None
```

**示例:**
```python
#!/usr/bin/env python3
"""TrackingSession 完整示例 - 保存为 ~/.config/bspterm/scripts/example_tracking.py"""

import time
from bspterm import current_terminal

term = current_terminal()

# 创建跟踪会话
tracker = term.track()

print(f"终端 ID: {tracker.terminal_id}")
print(f"跟踪器 ID: {tracker.reader_id}")

# 执行一系列命令并分别获取输出
commands = ["echo 'A'", "echo 'B'", "echo 'C'"]

for cmd in commands:
    term.send(f"{cmd}\n")
    time.sleep(0.3)
    output = tracker.read_new()
    print(f"'{cmd}' 的输出: {output.strip()}")

# 停止跟踪
tracker.stop()
print("跟踪已停止")
```

---

## SSH 连接

### SSH.connect()

创建后台 SSH 连接（无 UI 窗口）。

**签名:**
```python
@staticmethod
def connect(
    host: str,
    port: int = 22,
    user: str = None,
    password: str = None,
    private_key_path: str = None,
    passphrase: str = None
) -> Terminal
```

**参数:**
- `host` (str) - SSH 服务器主机名或 IP
- `port` (int) - SSH 端口，默认 22
- `user` (str) - 用户名
- `password` (str) - 密码
- `private_key_path` (str) - 私钥文件路径
- `passphrase` (str) - 私钥密码

**返回值:**
- `Terminal` - SSH 连接的终端实例

**示例:**
```python
#!/usr/bin/env python3
"""SSH 连接示例 - 保存为 ~/.config/bspterm/scripts/example_ssh.py"""

from bspterm import SSH, TimeoutError

# 使用密码认证
try:
    term = SSH.connect(
        host="192.168.1.100",
        port=22,
        user="admin",
        password="secret123"
    )

    # 等待 shell 就绪
    term.wait_for(r"[$#>]", timeout=10)

    # 执行命令
    hostname = term.sendcmd("hostname")
    uptime = term.sendcmd("uptime")

    print(f"主机: {hostname.strip()}")
    print(f"运行时间: {uptime.strip()}")

    # 关闭连接
    term.close()

except TimeoutError:
    print("连接超时")
except Exception as e:
    print(f"连接失败: {e}")
```

**使用密钥认证:**
```python
#!/usr/bin/env python3
"""SSH 密钥认证示例 - 保存为 ~/.config/bspterm/scripts/example_ssh_key.py"""

from bspterm import SSH
import os

term = SSH.connect(
    host="192.168.1.100",
    user="admin",
    private_key_path=os.path.expanduser("~/.ssh/id_rsa"),
    passphrase="key_password"  # 如果密钥有密码
)

term.wait_for(r"[$#>]", timeout=10)
result = term.sendcmd("whoami")
print(f"登录用户: {result.strip()}")

term.close()
```

---

## Telnet 连接

### Telnet.connect()

创建后台 Telnet 连接（无 UI 窗口）。

**签名:**
```python
@staticmethod
def connect(
    host: str,
    port: int = 23,
    username: str = None,
    password: str = None
) -> Terminal
```

**参数:**
- `host` (str) - Telnet 服务器主机名或 IP
- `port` (int) - Telnet 端口，默认 23
- `username` (str) - 用户名
- `password` (str) - 密码

**返回值:**
- `Terminal` - Telnet 连接的终端实例

**示例:**
```python
#!/usr/bin/env python3
"""Telnet 连接示例 - 保存为 ~/.config/bspterm/scripts/example_telnet.py"""

from bspterm import Telnet, TimeoutError

try:
    term = Telnet.connect(
        host="192.168.1.1",
        port=23,
        username="admin",
        password="admin123"
    )

    # 等待登录提示
    term.wait_for("login:", timeout=5)
    term.send("admin\n")

    term.wait_for("Password:", timeout=5)
    term.send("admin123\n")

    # 等待命令提示符
    term.wait_for(r"[#>]", timeout=5)

    # 执行命令
    result = term.sendcmd("show version", prompt_pattern=r"[#>]$")
    print(f"设备版本:\n{result}")

    term.close()

except TimeoutError as e:
    print(f"操作超时: {e}")
```

---

## 数据类

### Screen

终端屏幕内容。

| 属性 | 类型 | 描述 |
|------|------|------|
| `text` | str | 屏幕文本内容 |
| `cursor_row` | int | 光标行位置 |
| `cursor_col` | int | 光标列位置 |
| `rows` | int | 终端行数 |
| `cols` | int | 终端列数 |

### CommandResult

标记命令的执行结果。

| 属性 | 类型 | 描述 |
|------|------|------|
| `command_id` | str | 命令唯一标识符 |
| `output` | str | 命令输出 |
| `exit_code` | Optional[int] | 退出码 |

### SessionInfo

终端会话信息。

| 属性 | 类型 | 描述 |
|------|------|------|
| `id` | str | 会话 ID |
| `name` | str | 会话名称 |
| `type` | str | 会话类型 |
| `connected` | bool | 连接状态 |

**方法:**
- `connect() -> Terminal` - 获取该会话的 Terminal 实例

---

## 异常处理

### 异常类型

| 异常 | 描述 |
|------|------|
| `BsptermError` | 所有 BSPTerm 异常的基类 |
| `ConnectionError` | 无法连接到 BSPTerm 服务器 |
| `TerminalNotFoundError` | 指定的终端不存在 |
| `TimeoutError` | 操作超时 |
| `JsonRpcError` | JSON-RPC 协议错误 |

**示例:**
```python
#!/usr/bin/env python3
"""异常处理示例 - 保存为 ~/.config/bspterm/scripts/example_exceptions.py"""

from bspterm import (
    current_terminal,
    BsptermError,
    ConnectionError,
    TerminalNotFoundError,
    TimeoutError,
    JsonRpcError,
)

try:
    term = current_terminal()
    result = term.sendcmd("some_command", timeout=5)
    print(result)

except ConnectionError as e:
    print(f"连接错误: {e}")
    print("请确保 BSPTerm 正在运行")

except TerminalNotFoundError as e:
    print(f"终端未找到: {e}")
    print("请先打开或聚焦一个终端")

except TimeoutError as e:
    print(f"操作超时: {e}")
    print("命令执行时间过长或未检测到提示符")

except JsonRpcError as e:
    print(f"协议错误 [{e.code}]: {e.message}")

except BsptermError as e:
    print(f"BSPTerm 错误: {e}")
```

---

## 完整示例

### 示例 1: 批量设备配置

```python
#!/usr/bin/env python3
"""批量设备配置 - 保存为 ~/.config/bspterm/scripts/batch_config.py"""

from bspterm import SSH, TimeoutError
import time

# 设备列表
devices = [
    {"host": "192.168.1.1", "user": "admin", "password": "admin123"},
    {"host": "192.168.1.2", "user": "admin", "password": "admin123"},
    {"host": "192.168.1.3", "user": "admin", "password": "admin123"},
]

# 要执行的配置命令
config_commands = [
    "configure terminal",
    "hostname CONFIGURED",
    "ntp server 192.168.1.254",
    "end",
    "write memory",
]

results = []

for device in devices:
    print(f"\n{'=' * 50}")
    print(f"配置设备: {device['host']}")
    print('=' * 50)

    try:
        # 连接设备
        term = SSH.connect(
            host=device["host"],
            user=device["user"],
            password=device["password"]
        )

        # 等待登录
        term.wait_for(r"[#>]", timeout=10)

        # 执行配置命令
        for cmd in config_commands:
            output = term.sendcmd(cmd, prompt_pattern=r"[#>]$")
            print(f"  {cmd}: OK")

        results.append({"host": device["host"], "status": "SUCCESS"})
        term.close()

    except TimeoutError:
        results.append({"host": device["host"], "status": "TIMEOUT"})
        print(f"  错误: 连接超时")

    except Exception as e:
        results.append({"host": device["host"], "status": f"ERROR: {e}"})
        print(f"  错误: {e}")

# 打印汇总
print("\n" + "=" * 50)
print("配置汇总")
print("=" * 50)
for r in results:
    print(f"  {r['host']}: {r['status']}")
```

### 示例 2: 日志收集

```python
#!/usr/bin/env python3
"""日志收集 - 保存为 ~/.config/bspterm/scripts/collect_logs.py"""

from bspterm import current_terminal
import time
from datetime import datetime

term = current_terminal()

print("开始收集日志...")

# 收集各种日志
logs = {}

# 系统日志
logs["dmesg"] = term.sendcmd("dmesg | tail -50", timeout=10)

# 认证日志
logs["auth"] = term.sendcmd("sudo tail -50 /var/log/auth.log 2>/dev/null || echo 'N/A'", timeout=10)

# 系统状态
logs["processes"] = term.sendcmd("ps aux --sort=-%mem | head -20", timeout=10)
logs["memory"] = term.sendcmd("free -h", timeout=10)
logs["disk"] = term.sendcmd("df -h", timeout=10)
logs["network"] = term.sendcmd("netstat -tuln 2>/dev/null || ss -tuln", timeout=10)

# 生成报告
timestamp = datetime.now().strftime("%Y-%m-%d %H:%M:%S")
report = f"""
================================================================================
系统日志报告
生成时间: {timestamp}
================================================================================

--- 内存使用 ---
{logs['memory']}

--- 磁盘使用 ---
{logs['disk']}

--- 网络监听端口 ---
{logs['network']}

--- 内存占用最高的进程 ---
{logs['processes']}

--- 最近的系统日志 ---
{logs['dmesg']}

================================================================================
"""

print(report)

# 可选: 保存到文件
# with open(f"/tmp/system_report_{datetime.now().strftime('%Y%m%d_%H%M%S')}.txt", "w") as f:
#     f.write(report)
```

### 示例 3: 交互式菜单脚本

```python
#!/usr/bin/env python3
"""交互式设备管理 - 保存为 ~/.config/bspterm/scripts/device_menu.py"""

from bspterm import current_terminal, Session

def show_menu():
    print("\n" + "=" * 40)
    print("设备管理菜单")
    print("=" * 40)
    print("1. 查看系统信息")
    print("2. 查看网络状态")
    print("3. 查看磁盘使用")
    print("4. 查看进程列表")
    print("5. 列出所有终端")
    print("0. 退出")
    print("=" * 40)

def main():
    term = current_terminal()

    while True:
        show_menu()
        choice = input("请选择操作 [0-5]: ").strip()

        if choice == "0":
            print("退出脚本")
            break

        elif choice == "1":
            print("\n--- 系统信息 ---")
            print(f"主机名: {term.sendcmd('hostname').strip()}")
            print(f"内核: {term.sendcmd('uname -r').strip()}")
            print(f"运行时间: {term.sendcmd('uptime').strip()}")

        elif choice == "2":
            print("\n--- 网络状态 ---")
            print(term.sendcmd("ip addr show | grep -E 'inet |^[0-9]'"))

        elif choice == "3":
            print("\n--- 磁盘使用 ---")
            print(term.sendcmd("df -h"))

        elif choice == "4":
            print("\n--- 进程列表 (Top 10) ---")
            print(term.sendcmd("ps aux --sort=-%cpu | head -11"))

        elif choice == "5":
            print("\n--- 所有终端 ---")
            sessions = Session.list()
            for s in sessions:
                status = "已连接" if s.connected else "已断开"
                print(f"  [{s.type}] {s.name} - {status}")

        else:
            print("无效选择，请重试")

if __name__ == "__main__":
    main()
```

### 示例 4: 网络设备巡检

```python
#!/usr/bin/env python3
"""网络设备巡检 - 保存为 ~/.config/bspterm/scripts/network_check.py"""

from bspterm import Telnet, TimeoutError

# 华为交换机巡检
def check_huawei_switch(host, username, password):
    """华为交换机巡检"""
    results = {"host": host, "status": "OK", "data": {}}

    try:
        term = Telnet.connect(host=host, port=23)

        # 登录
        term.wait_for("Username:", timeout=5)
        term.send(f"{username}\n")
        term.wait_for("Password:", timeout=5)
        term.send(f"{password}\n")
        term.wait_for(r"[<>\[\]]", timeout=5)

        # 华为设备使用 <> 或 [] 作为提示符
        prompt = r"[<>\[\]].*[<>\[\]]$"

        # 收集信息
        results["data"]["version"] = term.sendcmd("display version", prompt_pattern=prompt)
        results["data"]["cpu"] = term.sendcmd("display cpu-usage", prompt_pattern=prompt)
        results["data"]["memory"] = term.sendcmd("display memory", prompt_pattern=prompt)
        results["data"]["interfaces"] = term.sendcmd("display interface brief", prompt_pattern=prompt)

        term.close()

    except TimeoutError:
        results["status"] = "TIMEOUT"
    except Exception as e:
        results["status"] = f"ERROR: {e}"

    return results

# 思科交换机巡检
def check_cisco_switch(host, username, password):
    """思科交换机巡检"""
    results = {"host": host, "status": "OK", "data": {}}

    try:
        term = Telnet.connect(host=host, port=23)

        # 登录
        term.wait_for("Username:", timeout=5)
        term.send(f"{username}\n")
        term.wait_for("Password:", timeout=5)
        term.send(f"{password}\n")
        term.wait_for(r"[#>]", timeout=5)

        prompt = r"\S+[#>]$"

        # 进入特权模式
        screen = term.read()
        if ">" in screen.text.split("\n")[-1]:
            term.send("enable\n")
            term.wait_for("Password:", timeout=5)
            term.send(f"{password}\n")
            term.wait_for("#", timeout=5)

        # 收集信息
        results["data"]["version"] = term.sendcmd("show version", prompt_pattern=prompt)
        results["data"]["cpu"] = term.sendcmd("show processes cpu", prompt_pattern=prompt)
        results["data"]["memory"] = term.sendcmd("show memory statistics", prompt_pattern=prompt)
        results["data"]["interfaces"] = term.sendcmd("show ip interface brief", prompt_pattern=prompt)

        term.close()

    except TimeoutError:
        results["status"] = "TIMEOUT"
    except Exception as e:
        results["status"] = f"ERROR: {e}"

    return results

# 主程序
if __name__ == "__main__":
    devices = [
        {"host": "192.168.1.1", "type": "huawei", "user": "admin", "pass": "admin123"},
        {"host": "192.168.1.2", "type": "cisco", "user": "admin", "pass": "admin123"},
    ]

    for device in devices:
        print(f"\n检查设备: {device['host']} ({device['type']})")

        if device["type"] == "huawei":
            result = check_huawei_switch(device["host"], device["user"], device["pass"])
        else:
            result = check_cisco_switch(device["host"], device["user"], device["pass"])

        print(f"状态: {result['status']}")
        if result["status"] == "OK":
            print(f"接口数量: {len(result['data'].get('interfaces', '').split(chr(10)))}")
```

---

## 最佳实践

1. **优先使用 `sendcmd()`**: 对于需要获取命令输出的场景，`sendcmd()` 返回最干净的结果。

2. **合理设置超时**: 根据命令的预期执行时间设置合适的超时值。

3. **使用上下文管理器**: 使用 `with term.track() as tracker:` 确保资源正确释放。

4. **异常处理**: 始终处理可能的异常，特别是 `TimeoutError` 和 `TerminalNotFoundError`。

5. **关闭连接**: 对于后台创建的 SSH/Telnet 连接，使用完毕后调用 `close()` 释放资源。

6. **自定义提示符**: 对于非标准 shell（如网络设备），提供正确的 `prompt_pattern`。

---

## 脚本目录

BSPTerm 脚本默认存放在 `~/.config/bspterm/scripts/` 目录下。所有 `.py` 文件都会自动显示在 Script Panel 中。

---

## 环境变量

| 变量 | 描述 |
|------|------|
| `BSPTERM_SOCKET` | Unix socket 路径，用于与 BSPTerm 通信 |
| `BSPTERM_CURRENT_TERMINAL` | 启动脚本时聚焦的终端 ID |
| `PYTHONPATH` | 自动设置为脚本目录，以便导入 `bspterm` 模块 |

---

## 版本信息

- **文档版本**: 1.0
- **适用于**: BSPTerm
- **更新日期**: 2026-02
