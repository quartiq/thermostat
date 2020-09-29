import socket
import json

class Client:
    def __init__(self, host="192.168.1.26", port=23, timeout=None):
        self._socket = socket.create_connection((host, port), timeout)
        self._lines = [""]

    def _command(self, *command):
        self._socket.sendall((" ".join(command) + "\n").encode('utf-8'))
        
    def _read_line(self):
        # read more lines
        while len(self._lines) <= 1:
            chunk = self._socket.recv(4096)
            if not chunk:
                return None
            buf = self._lines[-1] + chunk.decode('utf-8', errors='ignore')
            self._lines = buf.split("\n")

        line = self._lines[0]
        self._lines = self._lines[1:]
        return line
            
    def report_mode(self):
        """Start reporting measurement values

        
        """
        self._command("report mode", "on")
        self._read_line()

        while True:
            line = self._read_line()
            if not line:
                break
            try:
                yield json.loads(line)
            except json.decoder.JSONDecodeError:
                pass

    def set_param(self, topic, channel, field="", value=""):
        if type(value) is float:
            value = "{:f}".format(value)
        if type(value) is not str:
            value = str(value)
        self._command(topic, str(channel), field, value)

        # read response line
        self._read_line()

    def power_up(self, channel, target):
        self.set_param("pid", channel, "target", value=target)
        self.set_param("pwm", channel, "pid")
