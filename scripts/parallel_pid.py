import socket
import time
from coef import *
import matplotlib.pyplot as plt
import matplotlib.animation as animation
import datetime as dt

''' This script configures the thermostat iir matrix for two parallel PIDs using a PID IIR each.'''

target = 45  # temperature target (°C)

# gains in dB, freqs relative to f_sample
# gains in dB, freqs relative to f_sample
k_i = -30 # integral gain (at nyquist)
k_p = 10  # proportional gain
k_d = 40  # derivative gain (aka gain at nyquist)

ba_0 = pid(k_i, k_p, k_d)

s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
s.connect(('192.168.1.26', 23))
msg = ''


s.send('pwm 0 max_v 3\n'.encode())
time.sleep(0.1)
s.send('pwm 0 max_i_pos 0.8\n'.encode())
time.sleep(0.1)
s.send('pwm 0 max_i_neg 0.8\n'.encode())
time.sleep(0.1)
s.send('pwm 1 max_v 3\n'.encode())
time.sleep(0.1)
s.send('pwm 1 max_i_pos 0.8\n'.encode())
time.sleep(0.1)
s.send('pwm 1 max_i_neg 0.8\n'.encode())
time.sleep(0.1)
s.send('matrix target 0 val {}\n'.format(target).encode())
time.sleep(0.1)
s.send('matrix in 0 temp 0\n'.encode())
time.sleep(0.1)
s.send('iir 2 {:4.8f} {:4.8f} {:4.8f} {:4.8f} {:4.8f}\n'.format(ba_0[0], ba_0[1], ba_0[2], ba_0[3], ba_0[4]).encode())
time.sleep(0.1)
s.send('matrix target 1 val {}\n'.format(target).encode())
time.sleep(0.1)
s.send('matrix in 1 temp 1\n'.encode())
time.sleep(0.1)
s.send('iir 3 {:4.8f} {:4.8f} {:4.8f} {:4.8f} {:4.8f}\n'.format(ba_0[0], ba_0[1], ba_0[2], ba_0[3], ba_0[4]).encode())
time.sleep(0.1)
s.send('pwm 0 matrix 1\n'.encode())
time.sleep(0.1)
s.send('pwm 1 matrix 2\n'.encode())
time.sleep(0.1)
s.send('report\n'.encode())
msg = s.recv(1024).decode("utf-8")

print(msg)
s.close()
