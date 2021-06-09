import socket
import time
from coef import *
import matplotlib.pyplot as plt
import matplotlib.animation as animation
import datetime as dt

''' This script configures the thermostat iir matrix for a single channel 0 PID using two IIRs.'''

target = 45  # temperature target (°C)

# gains in dB, freqs relative to f_sample
k_i = 80  # integral gain (aka gain at DC)
fc_i = 0.0001  # integral gain cutoff
k_p = 10  # proportional gain
k_d = 40  # derivative gain (aka gain at nyquist)
fc_d = 0.01  # derivative gain cutoff

ba_0 = pi(fc_i, k_p/2, k_i)
ba_1 = pd(fc_d, k_p/2, k_d)

s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
s.connect(('192.168.1.26', 23))
msg = ''


s.send('pwm 0 max_v 3\n'.encode())
time.sleep(0.1)
s.send('pwm 0 max_i_pos 0.8\n'.encode())
time.sleep(0.1)
s.send('pwm 0 max_i_neg 0.8\n'.encode())
time.sleep(0.1)
s.send('matrix target 0 val {}\n'.format(target).encode())
time.sleep(0.1)
s.send('matrix in 0 temp 0\n'.encode())
time.sleep(0.1)
s.send('iir 2 {:4.8f} {:4.8f} {:4.8f} {:4.8f} {:4.8f}\n'.format(ba_0[0], ba_0[1], ba_0[2], ba_0[3], ba_0[4]).encode())
time.sleep(0.1)
s.send('matrix target 1 matrix 0\n'.encode())  # set target of iir1 to iir0
time.sleep(0.1)
s.send('iir 3 {:4.8f} {:4.8f} {:4.8f} {:4.8f} {:4.8f}\n'.format(ba_1[0], ba_1[1], ba_1[2], ba_1[3], ba_1[4]).encode())
time.sleep(0.1)
s.send('pwm 0 matrix 2\n'.encode())
time.sleep(0.1)
s.send('report\n'.encode())
msg = s.recv(1024).decode("utf-8")

print(msg)
s.close()