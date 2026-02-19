"""
BSPTerm - BSPTerm Python Terminal Automation Library

This module provides a Python client for automating terminal sessions
in BSPTerm. It communicates with the application via a Unix socket
using JSON-RPC 2.0 protocol.

Usage:
    from bspterm import current_terminal, Session, SSH, Telnet

    # Get the current focused terminal
    term = current_terminal()
    output = term.run("ls -la")
    print(output)

    # List all sessions
    sessions = Session.list()
    for s in sessions:
        print(f"{s.id}: {s.name}")
"""

import json
import os
import socket
import time
from dataclasses import dataclass
from typing import Optional, List, Dict, Any


class BsptermError(Exception):
    """Base exception for BSPTerm errors."""
    pass


class ConnectionError(BsptermError):
    """Error connecting to the BSPTerm server."""
    pass


class TerminalNotFoundError(BsptermError):
    """Terminal not found."""
    pass


class TimeoutError(BsptermError):
    """Operation timed out."""
    pass


class JsonRpcError(BsptermError):
    """JSON-RPC error from server."""
    def __init__(self, code: int, message: str, data: Any = None):
        self.code = code
        self.message = message
        self.data = data
        super().__init__(f"[{code}] {message}")


def _get_socket_path() -> str:
    """Get the socket path from environment or default location."""
    if "BSPTERM_SOCKET" in os.environ:
        return os.environ["BSPTERM_SOCKET"]

    runtime_dir = os.environ.get("XDG_RUNTIME_DIR")
    if not runtime_dir:
        runtime_dir = os.environ.get("TMPDIR", "/tmp")

    ppid = os.getppid()
    return os.path.join(runtime_dir, f"bspterm-{ppid}.sock")


class _RpcClient:
    """Low-level JSON-RPC client."""

    def __init__(self, socket_path: Optional[str] = None):
        self.socket_path = socket_path or _get_socket_path()
        self._socket: Optional[socket.socket] = None
        self._request_id = 0

    def connect(self):
        """Connect to the server."""
        if self._socket is not None:
            return

        self._socket = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        try:
            self._socket.connect(self.socket_path)
        except OSError as e:
            self._socket = None
            raise ConnectionError(f"Failed to connect to {self.socket_path}: {e}")

    def disconnect(self):
        """Disconnect from the server."""
        if self._socket is not None:
            self._socket.close()
            self._socket = None

    def call(self, method: str, params: Dict[str, Any] = None) -> Any:
        """Make a JSON-RPC call."""
        self.connect()

        self._request_id += 1
        request = {
            "jsonrpc": "2.0",
            "method": method,
            "params": params or {},
            "id": self._request_id,
        }

        request_json = json.dumps(request) + "\n"
        self._socket.sendall(request_json.encode("utf-8"))

        response_data = b""
        while True:
            chunk = self._socket.recv(4096)
            if not chunk:
                raise ConnectionError("Connection closed by server")
            response_data += chunk
            if b"\n" in response_data:
                break

        response_json = response_data.decode("utf-8").strip()
        response = json.loads(response_json)

        if "error" in response and response["error"]:
            err = response["error"]
            code = err.get("code", -1)
            message = err.get("message", "Unknown error")

            if code == -32000:
                raise TerminalNotFoundError(message)
            elif code == -32001:
                raise TimeoutError(message)
            else:
                raise JsonRpcError(code, message, err.get("data"))

        return response.get("result")


_client: Optional[_RpcClient] = None


def _get_client() -> _RpcClient:
    """Get or create the global RPC client."""
    global _client
    if _client is None:
        _client = _RpcClient()
    return _client


@dataclass
class Screen:
    """Terminal screen content."""
    text: str
    cursor_row: int
    cursor_col: int
    rows: int
    cols: int


@dataclass
class CommandResult:
    """Result of a marked command execution."""
    command_id: str
    output: str
    exit_code: Optional[int]


class TrackingSession:
    """
    Incremental output tracking session.

    Use this to track terminal output incrementally, only reading
    new content since the last read.

    Example:
        tracker = term.track()
        term.send("command1\\n")
        time.sleep(1)
        output1 = tracker.read_new()  # Only command1 output

        term.send("command2\\n")
        time.sleep(1)
        output2 = tracker.read_new()  # Only command2 output

        tracker.stop()
    """

    def __init__(self, terminal_id: str, reader_id: str):
        self._terminal_id = terminal_id
        self._reader_id = reader_id
        self._client = _get_client()
        self._stopped = False

    @property
    def terminal_id(self) -> str:
        """The terminal ID this tracker is attached to."""
        return self._terminal_id

    @property
    def reader_id(self) -> str:
        """The unique reader ID for this tracking session."""
        return self._reader_id

    def read_new(self) -> str:
        """
        Read new output since the last read.

        Returns:
            String containing only the new output since last read.
            Empty string if no new output.

        Raises:
            BsptermError: If the tracking session has been stopped.
        """
        if self._stopped:
            raise BsptermError("Tracking session has been stopped")

        result = self._client.call("terminal.track_read", {
            "terminal_id": self._terminal_id,
            "reader_id": self._reader_id,
        })
        return result["content"]

    def stop(self) -> None:
        """
        Stop the tracking session and release resources.

        After calling stop(), read_new() will raise an error.
        """
        if self._stopped:
            return

        self._client.call("terminal.track_stop", {
            "terminal_id": self._terminal_id,
            "reader_id": self._reader_id,
        })
        self._stopped = True

    def __enter__(self) -> "TrackingSession":
        return self

    def __exit__(self, exc_type, exc_val, exc_tb) -> None:
        self.stop()


class Terminal:
    """Represents a terminal session for automation."""

    def __init__(self, terminal_id: str, connected: bool = True, session_type: str = "local"):
        self.id = terminal_id
        self.connected = connected
        self.type = session_type
        self._client = _get_client()

    def send(self, data: str) -> None:
        """Send raw input to the terminal."""
        self._client.call("terminal.send", {
            "terminal_id": self.id,
            "data": data,
        })

    def read(self) -> Screen:
        """Read the current screen content."""
        result = self._client.call("terminal.read", {
            "terminal_id": self.id,
        })
        return Screen(
            text=result["text"],
            cursor_row=result["cursor_row"],
            cursor_col=result["cursor_col"],
            rows=result["rows"],
            cols=result["cols"],
        )

    def screen(self) -> Screen:
        """Alias for read()."""
        return self.read()

    def wait_for(self, pattern: str, timeout: float = 30.0) -> str:
        """
        Wait for a pattern to appear in the terminal output.

        Args:
            pattern: Regular expression pattern to match
            timeout: Timeout in seconds

        Returns:
            The screen content when pattern was matched

        Raises:
            TimeoutError: If pattern not found within timeout
        """
        result = self._client.call("terminal.wait_for", {
            "terminal_id": self.id,
            "pattern": pattern,
            "timeout_ms": int(timeout * 1000),
        })
        return result["content"]

    def run(self, command: str, timeout: float = 30.0, prompt_pattern: str = None) -> str:
        """
        Run a command and wait for it to complete.

        Args:
            command: Command to execute
            timeout: Timeout in seconds
            prompt_pattern: Regex pattern to detect command completion (default: shell prompt)

        Returns:
            Command output

        Raises:
            TimeoutError: If command does not complete within timeout
        """
        params = {
            "terminal_id": self.id,
            "command": command,
            "timeout_ms": int(timeout * 1000),
        }
        if prompt_pattern:
            params["prompt_pattern"] = prompt_pattern

        result = self._client.call("terminal.run", params)
        return result["output"]

    def sendcmd(
        self,
        command: str,
        timeout: float = 30.0,
        prompt_pattern: str = None
    ) -> str:
        """
        Execute a command and return clean output.

        Automatically strips:
        - Command echo (the command you typed)
        - Shell prompt

        Args:
            command: Command to execute
            timeout: Timeout in seconds
            prompt_pattern: Regex pattern to detect prompt (default: [$#>]\\s*$)

        Returns:
            Clean command output without echo or prompt

        Raises:
            TimeoutError: If command does not complete within timeout

        Example:
            result = term.sendcmd("ls -la")
            print(result)  # Only file listing, no "ls -la" echo, no prompt
        """
        params = {
            "terminal_id": self.id,
            "command": command,
            "timeout_ms": int(timeout * 1000),
            "strip_echo": True,
        }
        if prompt_pattern:
            params["prompt_pattern"] = prompt_pattern

        result = self._client.call("terminal.sendcmd", params)
        return result["output"]

    def close(self) -> None:
        """Close the terminal connection."""
        self._client.call("terminal.close", {
            "terminal_id": self.id,
        })

    def track(self) -> TrackingSession:
        """
        Start incremental output tracking.

        Returns a TrackingSession that allows reading only new output
        since the last read, enabling precise tracking of command outputs.

        Returns:
            TrackingSession instance for incremental reading

        Example:
            tracker = term.track()
            term.send("ls\\n")
            time.sleep(1)
            output = tracker.read_new()  # Only ls output
            tracker.stop()

            # Or use as context manager:
            with term.track() as tracker:
                term.send("pwd\\n")
                time.sleep(1)
                output = tracker.read_new()
        """
        result = self._client.call("terminal.track_start", {
            "terminal_id": self.id,
        })
        return TrackingSession(self.id, result["reader_id"])

    def run_marked(
        self,
        command: str,
        timeout: float = 30.0,
        prompt_pattern: str = None
    ) -> CommandResult:
        """
        Execute a command and precisely capture its output.

        Unlike run(), this method tracks the exact boundaries of the
        command's output using internal markers, allowing for more
        accurate output capture.

        Args:
            command: Command to execute
            timeout: Timeout in seconds
            prompt_pattern: Regex pattern to detect command completion

        Returns:
            CommandResult with command_id, output, and exit_code

        Raises:
            TimeoutError: If command does not complete within timeout
        """
        params = {
            "terminal_id": self.id,
            "command": command,
            "timeout_ms": int(timeout * 1000),
        }
        if prompt_pattern:
            params["prompt_pattern"] = prompt_pattern

        result = self._client.call("terminal.run_marked", params)
        return CommandResult(
            command_id=result["command_id"],
            output=result["output"],
            exit_code=result.get("exit_code"),
        )

    def read_time_range(self, start_seconds: float, end_seconds: float) -> str:
        """
        Read output within a time range.

        Time is measured from when tracking started for this terminal.

        Args:
            start_seconds: Start time in seconds from tracking start
            end_seconds: End time in seconds from tracking start

        Returns:
            String containing output within the specified time range
        """
        result = self._client.call("terminal.read_time_range", {
            "terminal_id": self.id,
            "start_ms": int(start_seconds * 1000),
            "end_ms": int(end_seconds * 1000),
        })
        return result["content"]


@dataclass
class SessionInfo:
    """Information about a terminal session."""
    id: str
    name: str
    type: str
    connected: bool

    def connect(self) -> Terminal:
        """Get a Terminal instance for this session."""
        return Terminal(self.id, self.connected, self.type)


class Session:
    """Session management."""

    @staticmethod
    def list() -> List[SessionInfo]:
        """List all terminal sessions."""
        client = _get_client()
        result = client.call("session.list")
        return [
            SessionInfo(
                id=s["id"],
                name=s["name"],
                type=s["type"],
                connected=s["connected"],
            )
            for s in result
        ]

    @staticmethod
    def get(terminal_id: str) -> Terminal:
        """Get a terminal by ID."""
        client = _get_client()
        result = client.call("session.current", {"terminal_id": terminal_id})
        return Terminal(
            result["id"],
            result.get("connected", True),
            result.get("type", "unknown"),
        )


def current_terminal() -> Terminal:
    """
    Get the current focused terminal.

    When a script is launched from the Script Panel, this returns
    the terminal that was focused when the script was started.

    Returns:
        Terminal instance for the current/focused terminal

    Raises:
        TerminalNotFoundError: If no terminal is focused
    """
    terminal_id = os.environ.get("BSPTERM_CURRENT_TERMINAL")

    client = _get_client()
    params = {}
    if terminal_id:
        params["terminal_id"] = terminal_id

    result = client.call("session.current", params)
    return Terminal(
        result["id"],
        result.get("connected", True),
        result.get("type", "unknown"),
    )


def new_terminal(ssh: Dict[str, Any] = None, telnet: Dict[str, Any] = None) -> Terminal:
    """
    Create a new visible terminal window.

    Args:
        ssh: SSH connection parameters (host, port, username, password, etc.)
        telnet: Telnet connection parameters (host, port, username, password)

    Returns:
        Terminal instance for the new terminal

    Note:
        This function is not yet implemented.
    """
    client = _get_client()
    params = {}
    if ssh:
        params["ssh"] = ssh
    if telnet:
        params["telnet"] = telnet

    result = client.call("session.new_terminal", params)
    return Terminal(
        result["id"],
        result.get("connected", True),
        result.get("type", "unknown"),
    )


class SSH:
    """SSH connection factory for background connections."""

    @staticmethod
    def connect(
        host: str,
        port: int = 22,
        user: str = None,
        password: str = None,
        private_key_path: str = None,
        passphrase: str = None,
    ) -> Terminal:
        """
        Create a background SSH connection (no UI window).

        Args:
            host: SSH server hostname or IP
            port: SSH port (default: 22)
            user: Username for authentication
            password: Password for authentication
            private_key_path: Path to private key file
            passphrase: Passphrase for private key

        Returns:
            Terminal instance for the SSH connection

        Note:
            This function is not yet implemented.
        """
        client = _get_client()
        params = {
            "host": host,
            "port": port,
        }
        if user:
            params["username"] = user
        if password:
            params["password"] = password
        if private_key_path:
            params["private_key_path"] = private_key_path
        if passphrase:
            params["passphrase"] = passphrase

        result = client.call("session.create_ssh", params)
        return Terminal(
            result["id"],
            result.get("connected", True),
            "ssh",
        )


class Telnet:
    """Telnet connection factory for background connections."""

    @staticmethod
    def connect(
        host: str,
        port: int = 23,
        username: str = None,
        password: str = None,
    ) -> Terminal:
        """
        Create a background Telnet connection (no UI window).

        Args:
            host: Telnet server hostname or IP
            port: Telnet port (default: 23)
            username: Username for login
            password: Password for login

        Returns:
            Terminal instance for the Telnet connection

        Note:
            This function is not yet implemented.
        """
        client = _get_client()
        params = {
            "host": host,
            "port": port,
        }
        if username:
            params["username"] = username
        if password:
            params["password"] = password

        result = client.call("session.create_telnet", params)
        return Terminal(
            result["id"],
            result.get("connected", True),
            "telnet",
        )


__all__ = [
    "BsptermError",
    "ConnectionError",
    "TerminalNotFoundError",
    "TimeoutError",
    "JsonRpcError",
    "Screen",
    "CommandResult",
    "TrackingSession",
    "Terminal",
    "SessionInfo",
    "Session",
    "SSH",
    "Telnet",
    "current_terminal",
    "new_terminal",
]
