#!/usr/bin/env python3
"""Finite, private-X11 smoke: real native windows/input; no installed profile or network."""
import argparse
import os
from pathlib import Path
import signal
import subprocess
import tempfile
import time


def main():
    parser = argparse.ArgumentParser(__doc__)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--lifecycle", action="store_true", help="Requires a keyword-only binary; exercises real discovery/launch and hidden-instance recovery")
    args = parser.parse_args()
    binary = args.binary.resolve()
    args.output.mkdir(parents=True, exist_ok=False)
    with tempfile.TemporaryDirectory(prefix="findanything-smoke-") as temporary:
        # Preserve image MIME detection without exposing installed desktop entries
        # to discovery. GdkPixbuf needs this database to load embedded SVG icons.
        (Path(temporary) / "mime").symlink_to("/usr/share/mime", target_is_directory=True)
        env = dict(os.environ, HOME=temporary, XDG_DATA_HOME=temporary, XDG_CONFIG_HOME=temporary,
                   XDG_RUNTIME_DIR=temporary, XDG_DATA_DIRS=temporary, LIBGL_ALWAYS_SOFTWARE="1",
                   GDK_BACKEND="x11", GSK_RENDERER="cairo")
        env.pop("WAYLAND_DISPLAY", None)
        processes = []
        logs = []
        try:
            def start(command, name):
                log = open(Path(temporary) / (name + ".log"), "w")
                logs.append(log)
                process = subprocess.Popen(command, env=env, stdout=log, stderr=log, start_new_session=True)
                processes.append(process)
                return process

            def stop(process):
                # AppImage launchers can have a separate native child. Stop the
                # whole test-owned group so later fixtures cannot find its window.
                try:
                    os.killpg(process.pid, signal.SIGTERM)
                except ProcessLookupError:
                    pass
                process.wait(timeout=5)

            display_file = Path(temporary) / "display"
            with display_file.open("w") as display:
                xvfb = subprocess.Popen(["Xvfb", "-displayfd", str(display.fileno()), "-screen", "0", "1280x800x24", "-nolisten", "tcp"],
                                        pass_fds=(display.fileno(),), stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True)
                processes.append(xvfb)
                for _ in range(100):
                    if display_file.read_text().strip(): break
                    time.sleep(.05)
                env["DISPLAY"] = ":" + display_file.read_text().strip()
            # The tested app sees only disposable desktop entries. The window
            # manager still needs the distribution's installed theme resources.
            env["XDG_DATA_DIRS"] = "/usr/local/share:/usr/share"
            wm = start(["openbox"], "window-manager")
            env["XDG_DATA_DIRS"] = temporary
            for _ in range(100):
                if wm.poll() is not None:
                    raise RuntimeError(Path(logs[-1].name).read_text())
                ready = subprocess.run(["xprop", "-root", "_NET_SUPPORTING_WM_CHECK"], env=env, capture_output=True, text=True)
                if "window id #" in ready.stdout: break
                time.sleep(.05)
            else:
                raise RuntimeError("Window manager did not become ready: " + Path(logs[-1].name).read_text())

            def command(*args):
                return subprocess.check_output(args, env=env, text=True).strip()

            for theme in ["dark", "light"]:
                for state in ["results", "empty", "error"]:
                    app = start([str(binary), "--fixture", *([] if state == "results" else [state]), "--theme", theme], f"{theme}-{state}")
                    window = ""
                    for _ in range(200):
                        if app.poll() is not None:
                            raise RuntimeError(f"native app exited: {app.returncode}\n" + Path(logs[-1].name).read_text())
                        found = subprocess.run(["xdotool", "search", "--onlyvisible", "--name", "^Find Anything$"], env=env, capture_output=True, text=True)
                        if found.returncode == 0:
                            window = found.stdout.strip().splitlines()[-1]
                            break
                        time.sleep(.05)
                    assert window, "native window did not appear"
                    command("xdotool", "windowactivate", "--sync", window)
                    time.sleep(.8)
                    command("import", "-window", window, str(args.output / f"{theme}-{state}.png"))
                    canvas = command("convert", str(args.output / f"{theme}-{state}.png"), "-format", "%[hex:p{10,300}]", "info:")
                    assert canvas.upper().startswith("161618"), f"Graphite canvas changed under {theme}: {canvas}"
                    if state == "results":
                        command("xdotool", "key", "--clearmodifiers", "Down")
                        time.sleep(.3)
                        command("import", "-window", window, str(args.output / f"{theme}-selected.png"))
                        command("xdotool", "type", "--clearmodifiers", "display")
                        time.sleep(.4)
                        command("import", "-window", window, str(args.output / f"{theme}-query.png"))
                        assert (args.output / f"{theme}-query.png").read_bytes() != (args.output / f"{theme}-results.png").read_bytes()
                        command("xdotool", "mousemove", "--window", window, "716", "44", "click", "1")
                        time.sleep(.3)
                        # GTK popovers use a separate X11 surface; include the
                        # composited popup, not just the parent window pixmap.
                        menu_capture = str(args.output / f"{theme}-menu.png")
                        command("import", "-window", "root", menu_capture)
                        command("xdotool", "key", "--clearmodifiers", "Escape")
                        time.sleep(.2)
                        assert command("xdotool", "getwindowfocus") == window, "Escape in menu must keep the launcher open"
                        command("xdotool", "mousemove", "--window", window, "668", "44", "click", "1")
                        time.sleep(.4)
                        cleared = args.output / f"{theme}-cleared.png"
                        command("import", "-window", window, str(cleared))
                        # The third (file) row disappears when filtering and must
                        # return when the native clear image is clicked.
                        def file_row(path):
                            return command("convert", str(path), "-crop", "650x50+30+214", "+repage", "-format", "%#", "info:")
                        expected = file_row(args.output / f"{theme}-results.png")
                        assert file_row(args.output / f"{theme}-query.png") != expected
                        assert file_row(cleared) == expected, "Clear must restore all results"
                    command("xdotool", "windowsize", window, "640", "420")
                    time.sleep(.3)
                    command("import", "-window", window, str(args.output / f"{theme}-{state}-minimum.png"))
                    assert app.poll() is None
                    stop(app)
                    for _ in range(100):
                        remaining = subprocess.run(["xdotool", "search", "--onlyvisible", "--name", "^Find Anything$"], env=env, capture_output=True)
                        if remaining.returncode != 0: break
                        time.sleep(.05)
                    else:
                        raise AssertionError("Fixture window remained after process cleanup")
            print("PASS: 6 native fixture windows, Graphite palette under light/dark, keyboard selection/search, menu Escape, minimum size; screenshots captured")
            if args.lifecycle:
                applications = Path(temporary) / "applications"
                applications.mkdir()
                marker = Path(temporary) / "opened"
                (applications / "smoke.desktop").write_text(f"[Desktop Entry]\nType=Application\nName=Smoke Marker\nExec=/usr/bin/touch {marker}\n")
                app = start([str(binary), "--theme", "dark"], "live")

                def visible():
                    result = subprocess.run(["xdotool", "search", "--onlyvisible", "--name", "^Find Anything$"], env=env, capture_output=True, text=True)
                    return result.stdout.strip().splitlines() if result.returncode == 0 else []

                def wait_for(predicate, description):
                    for _ in range(100):
                        if app.poll() is not None: raise RuntimeError(Path(logs[-1].name).read_text())
                        if predicate(): return
                        time.sleep(.1)
                    raise AssertionError("Timed out: " + description)

                wait_for(visible, "initial live window")
                command("xdotool", "windowactivate", "--sync", visible()[-1])
                command("xdotool", "type", "--clearmodifiers", "smoke")
                time.sleep(1)
                command("xdotool", "key", "--clearmodifiers", "Return")
                wait_for(marker.exists, "real GIO launch")
                wait_for(lambda: not visible(), "hide after launch")
                secondary = subprocess.run([str(binary)], env=env, capture_output=True, timeout=10)
                assert secondary.returncode == 0, secondary.stderr
                wait_for(visible, "secondary launch restores owner")
                command("xdotool", "key", "--clearmodifiers", "Escape")
                wait_for(lambda: not visible(), "Escape hides owner")
                command("xdotool", "key", "--clearmodifiers", "ctrl+shift+space")
                wait_for(visible, "global shortcut restores owner")
                command("import", "-window", visible()[-1], str(args.output / "live-reopened.png"))
                print("PASS: real discovery/launch, hide on success, secondary-instance focus, Escape and global shortcut recovery")
        except Exception:
            for log in logs:
                print(Path(log.name).read_text(), flush=True)
            raise
        finally:
            for process in reversed(processes):
                stop(process)
            for log in logs:
                log.close()


if __name__ == "__main__":
    main()
