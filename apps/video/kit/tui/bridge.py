"""Runs a program in a pseudo-terminal of a given size and relays it, as a
terminal window would: what the program writes goes to standard output,
byte for byte, and standard input goes to the program as typed.

    python3 -I bridge.py COLUMNS ROWS WIDTH HEIGHT PROGRAM [ARGUMENT...]

WIDTH and HEIGHT are the terminal's size in pixels, which programs that
draw pictures ask for. It ends when the program does.
"""

import fcntl
import os
import pty
import select
import signal
import struct
import sys
import termios


def main():
    columns, rows, width, height = (int(value) for value in sys.argv[1:5])
    program = sys.argv[5:]
    size = struct.pack("HHHH", rows, columns, width, height)
    pid, terminal = pty.fork()
    if pid == 0:
        # The program's own side: sized before it starts, so its first
        # frame is drawn at the size it keeps.
        fcntl.ioctl(sys.stdin.fileno(), termios.TIOCSWINSZ, size)
        os.execvp(program[0], program)
    fcntl.ioctl(terminal, termios.TIOCSWINSZ, size)
    given = sys.stdin.fileno()
    shown = sys.stdout.fileno()
    while True:
        ready, _, _ = select.select([terminal, given], [], [])
        if terminal in ready:
            try:
                data = os.read(terminal, 65536)
            except OSError:
                break
            if not data:
                break
            os.write(shown, data)
        if given in ready:
            data = os.read(given, 65536)
            if not data:
                os.kill(pid, signal.SIGHUP)
                break
            os.write(terminal, data)
    os.waitpid(pid, 0)


main()
