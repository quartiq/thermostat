import socket
import time
from coef import *

''' This script configures the thermostat iir matrix for two pid loops with a
 crossover iir that compenstes for the thermal coupling of the TECs.'''

target = 45  # temperature target (°C)

# gains in dB, freqs relative to f_sample
k_i = -40 # integral gain (at nyquist)
k_p = 0  # proportional gain
k_d = 30  # derivative gain (aka gain at nyquist)

ba_0 = pid(k_i, k_p, k_d)

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
s.send('iir 2 {:4.8f} {:4.8f} {:4.8f} {:4.8f} {:4.8f}\n'.format(ba_0[0], ba_0[1], ba_0[2], ba_0[3], ba_0[4]).encode())
time.sleep(0.1)
s.send('matrix target 1 val {}\n'.format(target).encode())
time.sleep(0.1)
s.send('matrix in 1 temp 1\n'.encode())
time.sleep(0.1)
s.send('iir 3 {:4.8f} {:4.8f} {:4.8f} {:4.8f} {:4.8f}\n'.format(ba_0[0], ba_0[1], ba_0[2], ba_0[3], ba_0[4]).encode())
time.sleep(0.1)
s.send('matrix target 2 val 40\n'.encode())
time.sleep(0.1)
s.send('matrix in 2 temp 0\n'.encode())
time.sleep(0.1)
s.send('iir 3 0.000555 0 0 0.9967 0\n'.encode())     # simple first order lowpass for cross coupling
# s.send('iir 4 0.1 0 0 0 0\n'.encode())
time.sleep(0.1)
# setup fourth iir just as a summing (subtraction) junction
s.send('matrix in 3 matrix 2\n'.encode())
time.sleep(0.1)
s.send('matrix target 3 matrix 1\n'.encode())
time.sleep(0.1)
s.send('iir 5 1 0 0 0 0\n'.encode())
time.sleep(0.1)
s.send('pwm 0 matrix 1\n'.encode())
time.sleep(0.1)
s.send('pwm 1 matrix 4\n'.encode())
time.sleep(0.1)
s.send('report\n'.encode())
msg = s.recv(1024).decode("utf-8")

print(msg)
s.close()
