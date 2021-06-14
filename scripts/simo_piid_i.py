import socket
import time
from coef import *

''' This script configures the thermostat iir matrix for a single temp input,
dual TEC output with one PIID controlled, the other with another pure I from the
output of the first. (aka for very slow changes)'''

target = 45  # temperature target (°C)

# gains in dB, freqs relative to f_sample
k_i1 = 1000  # integral gain 1 (aka gain at DC)
fc_i1 = 0.002  # integral gain cutoff 1
k_i2 = 1000  # integral gain 2 (aka gain at DC)
fc_i2 = 0.002  # integral gain cutoff 2
k_p = 0  # proportional gain
k_d = 30  # derivative gain (aka gain at nyquist)
fc_d = 0.01  # derivative gain cutoff


ba_0 = pi(fc_i1, k_p/2, k_i1)
ba_1 = pi(fc_i2, 0, k_i2)
ba_2 = pd(fc_d, k_p/2, k_d)


s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
s.connect(('192.168.1.26', 23))
msg = ''


s.send('pwm 0 max_v 1.5\n'.encode())
time.sleep(0.1)
s.send('pwm 0 max_i_pos 0.8\n'.encode())
time.sleep(0.1)
s.send('pwm 0 max_i_neg 0.8\n'.encode())
time.sleep(0.1)
s.send('pwm 1 max_v 1.5\n'.encode())
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
s.send('matrix target 1 matrix 0\n'.encode())  # set target of iir1 to iir0
time.sleep(0.1)
s.send('iir 3 {:4.8f} {:4.8f} {:4.8f} {:4.8f} {:4.8f}\n'.format(ba_1[0], ba_1[1], ba_1[2], ba_1[3], ba_1[4]).encode())
time.sleep(0.1)
s.send('matrix target 2 matrix 1\n'.encode())  # set target of iir2 to iir1
time.sleep(0.1)
s.send('iir 4 {:4.8f} {:4.8f} {:4.8f} {:4.8f} {:4.8f}\n'.format(ba_2[0], ba_2[1], ba_2[2], ba_2[3], ba_2[4]).encode())
time.sleep(0.1)
s.send('matrix target 3 matrix 2\n'.encode())  # set target of iir3 to iir2
time.sleep(0.1)
s.send('iir 5 0.02 0.02 0 1 0\n'.encode())
time.sleep(0.1)
s.send('pwm 0 matrix 3\n'.encode())  # set pwm 0 to PIID output
time.sleep(0.1)
s.send('pwm 1 matrix 4\n'.encode())  # set pwm 0 to pure I output
time.sleep(0.1)
s.send('report\n'.encode())
msg = s.recv(1024).decode("utf-8")

print(msg)
s.close()
