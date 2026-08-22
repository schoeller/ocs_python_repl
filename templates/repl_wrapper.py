# Generated from templates/repl_wrapper.py by ocs_python_repl/build.rs
"""Platform wrapper that launches IPython (or plain Python) in an OS terminal."""
import os
import shlex
import shutil
import signal
import subprocess
import sys
import time
import traceback

SESSION_DIR = os.path.dirname(os.path.abspath(__file__))
PYTHON = sys.executable
LOG_PATH = os.path.join(SESSION_DIR, "repl.log")
STDERR_LOG_PATH = os.path.join(SESSION_DIR, "wrapper_stderr.log")

# Redirect the wrapper's Python-level stderr to a log file so the host can read
# startup errors. On Windows the wrapper console is hidden from the start.
try:
    sys.stderr = open(STDERR_LOG_PATH, "a", encoding="utf-8")
except Exception:
    pass


def log(msg):
    try:
        with open(LOG_PATH, "a", encoding="utf-8") as f:
            f.write(f"{time.strftime('%Y-%m-%d %H:%M:%S')} {msg}\n")
    except Exception:
        pass


log(f"wrapper started: python={PYTHON} platform={sys.platform}")

# Record our own PID so the host can target this process exactly.
try:
    with open(os.path.join(SESSION_DIR, "wrapper.pid"), "w", encoding="utf-8") as f:
        f.write(str(os.getpid()))
except Exception:
    pass


def check_ipython():
    try:
        import importlib.util

        return importlib.util.find_spec("IPython") is not None
    except Exception:
        return False


def make_env():
    env = os.environ.copy()
    parts = env.get("PYTHONPATH", "").split(os.pathsep)
    if SESSION_DIR not in parts:
        parts.insert(0, SESSION_DIR)
    env["PYTHONPATH"] = os.pathsep.join(parts)
    return env


def start_ipython_command():
    startup = os.path.join(SESSION_DIR, "startup.py")
    return [PYTHON, "-m", "IPython", "-i", startup]


def start_plain_command():
    startup = os.path.join(SESSION_DIR, "startup.py")
    return [PYTHON, "-i", startup]


def find_terminal():
    candidates = [
        "xterm",
        "konsole",
        "xfce4-terminal",
        "gnome-terminal",
        "alacritty",
        "kitty",
        "wezterm",
    ]
    for name in candidates:
        if shutil.which(name):
            return name
    if sys.platform == "darwin" and os.path.isdir(
        "/System/Applications/Utilities/Terminal.app"
    ):
        return "Terminal.app"
    return None


def build_terminal_command(term, cmd):
    if term == "xterm":
        return ["xterm", "-e"] + cmd
    if term == "konsole":
        return ["konsole", "-e"] + cmd
    if term == "xfce4-terminal":
        return ["xfce4-terminal", "-e", " ".join(shlex.quote(c) for c in cmd)]
    if term == "gnome-terminal":
        return ["gnome-terminal", "--"] + cmd
    if term == "alacritty":
        return ["alacritty", "-e"] + cmd
    if term == "kitty":
        return ["kitty"] + cmd
    if term == "wezterm":
        return ["wezterm", "start", "--"] + cmd
    raise RuntimeError(f"unsupported terminal: {term}")


def spawn_terminal_app(cmd, env):
    """Best-effort Terminal.app support on macOS."""
    import tempfile

    script = tempfile.NamedTemporaryFile(mode="w", suffix=".command", delete=False)
    pid_file = script.name + ".pid"

    def _cleanup():
        try:
            os.unlink(pid_file)
        except FileNotFoundError:
            pass
        except Exception:
            pass
        try:
            os.unlink(script.name)
        except FileNotFoundError:
            pass
        except Exception:
            pass

    try:
        script.write("#!/bin/sh\n")
        script.write(f"echo $$ > {shlex.quote(pid_file)}\n")
        for key, value in env.items():
            script.write(f"export {shlex.quote(key)}={shlex.quote(value)}\n")
        script.write("cd " + shlex.quote(SESSION_DIR) + "\n")
        script.write(" ".join(shlex.quote(c) for c in cmd) + "\n")
        script.write(f"rm -f {shlex.quote(pid_file)}\n")
        script.write(f"rm -f {shlex.quote(script.name)}\n")
        script.close()
        os.chmod(script.name, 0o755)

        try:
            subprocess.Popen(
                ["open", "-a", "Terminal.app", script.name], env=env
            )
        except Exception as e:
            print(f"failed to launch Terminal.app: {e}", file=sys.stderr)
            _cleanup()
            return _DummyProcess()

        # Wait for the shell inside Terminal.app to write its PID.
        for _ in range(50):
            if os.path.exists(pid_file):
                break
            time.sleep(0.1)

        if not os.path.exists(pid_file):
            _cleanup()
            return _DummyProcess()

        pid = int(open(pid_file).read())

        class Proc:
            def terminate(self):
                _kill_pid(pid, signal.SIGTERM)

            def kill(self):
                _kill_pid(pid, signal.SIGKILL)

            def wait(self):
                while True:
                    try:
                        os.kill(pid, 0)
                    except ProcessLookupError:
                        break
                    time.sleep(0.5)
                _cleanup()
                return 0

        return Proc()
    except Exception:
        _cleanup()
        raise


class _DummyProcess:
    def terminate(self):
        pass

    def kill(self):
        pass

    def wait(self):
        while True:
            time.sleep(1)


def _kill_pid(pid, sig):
    try:
        os.kill(pid, sig)
    except ProcessLookupError:
        pass
    except Exception:
        pass


def spawn_unix(cmd, env):
    term = find_terminal()
    if term is None:
        sys.exit(
            "No terminal emulator found. "
            "Install xterm, konsole, xfce4-terminal, gnome-terminal, alacritty, kitty, or wezterm."
        )
    if term == "Terminal.app":
        return spawn_terminal_app(cmd, env)
    return subprocess.Popen(build_terminal_command(term, cmd), env=env)


def install_signal_handlers(proc):
    def handler(signum, frame):
        try:
            proc.terminate()
        except Exception:
            pass
        try:
            proc.kill()
        except Exception:
            pass
        sys.exit(0)

    signal.signal(signal.SIGTERM, handler)
    signal.signal(signal.SIGINT, handler)


def install_windows_signal_handlers(proc):
    """Keep the wrapper alive while IPython handles Ctrl+C, and shut down the
    child when the host sends Ctrl+Break to the shared console."""

    def _ignore(signum, frame):
        pass

    def _shutdown(signum, frame):
        try:
            proc.terminate()
        except Exception:
            pass
        sys.exit(0)

    signal.signal(signal.SIGINT, _ignore)
    signal.signal(signal.SIGBREAK, _shutdown)


def main():
    try:
        if check_ipython():
            cmd = start_ipython_command()
            log("using IPython")
        else:
            log("IPython not found; falling back to plain Python REPL")
            print(
                "IPython not found; falling back to plain Python REPL.", file=sys.stderr
            )
            cmd = start_plain_command()

        env = make_env()
        log(f"command: {cmd}")
        log(f"PYTHONPATH: {env.get('PYTHONPATH', '')}")

        if sys.platform == "win32":
            # The host launches the wrapper with CREATE_NEW_CONSOLE, so the
            # wrapper already owns a visible console. IPython inherits that
            # console (no CREATE_NEW_CONSOLE). The wrapper ignores Ctrl+C so
            # only IPython handles it, and catches Ctrl+Break to shut down the
            # whole session when the host requests it.
            proc = subprocess.Popen(cmd, env=env)
            install_windows_signal_handlers(proc)
            log(f"spawned windows process pid={proc.pid}")
            try:
                with open(
                    os.path.join(SESSION_DIR, "ipython.pid"), "w", encoding="utf-8"
                ) as f:
                    f.write(str(proc.pid))
            except Exception:
                pass
        else:
            proc = spawn_unix(cmd, env)
            try:
                with open(os.path.join(SESSION_DIR, "ipython.pid"), "w", encoding="utf-8") as f:
                    f.write(str(proc.pid))
            except Exception:
                pass
            install_signal_handlers(proc)
            log(f"spawned unix process pid={proc.pid}")

        try:
            code = proc.wait()
            log(f"child exited with code {code}")
        except KeyboardInterrupt:
            log("keyboard interrupt")
            code = 0
        sys.exit(code)
    except Exception as e:
        log(f"wrapper crashed: {e}\n{traceback.format_exc()}")
        raise


if __name__ == "__main__":
    try:
        main()
    except Exception:
        traceback.print_exc()
        sys.exit(1)
