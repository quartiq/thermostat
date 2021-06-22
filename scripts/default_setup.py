import socket
import time
from coef import *
import matplotlib.pyplot as plt
import matplotlib.animation as animation
import datetime as dt

''' This script configures the thermostat iir matrix for two parallel PIDs using a PID IIR each.'''

target = 45  # temperature target (°C)


s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
s.connect(('192.168.1.26', 23))
msg = ''


s.send('pwm 0 max_v 3\n'.encode())
time.sleep(0.1)
s.send('pwm 0 max_i_pos 0.5\n'.encode())
time.sleep(0.1)
s.send('pwm 0 max_i_neg 0.5\n'.encode())
time.sleep(0.1)
s.send('pwm 1 max_v 3\n'.encode())
time.sleep(0.1)
s.send('pwm 1 max_i_pos 0.5\n'.encode())
time.sleep(0.1)
s.send('pwm 1 max_i_neg 0.5\n'.encode())
time.sleep(0.1)
s.send('matrix target 0 val {}\n'.format(target).encode())
time.sleep(0.1)
s.send('matrix in 0 temp 0\n'.encode())
time.sleep(0.1)
s.send('iir 2 1 0 0 0 0\n'.encode())
time.sleep(0.1)
s.send('matrix target 1 val {}\n'.format(target).encode())
time.sleep(0.1)
s.send('matrix in 1 temp 1\n'.encode())
time.sleep(0.1)
s.send('iir 3 1 0 0 0 0\n'.encode())
time.sleep(0.1)
s.send('pwm 0 matrix 1\n'.encode())
time.sleep(0.1)
s.send('pwm 1 matrix 2\n'.encode())
time.sleep(0.1)
s.send('report\n'.encode())
msg = s.recv(1024).decode("utf-8")

print(msg)
s.close()
