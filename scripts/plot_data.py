
import csv
import sys
import numpy as np
import matplotlib.pyplot as plt


time = []
temp1 = []
curr1 = []
temp2 = []
curr2 = []

with open(sys.argv[1], "r") as f:
    reader = csv.reader(f, delimiter=',')
    lc = 0
    for row in reader:
        if lc == 0:
            print(f'colums: {row[0]}, {row[1]}, {row[2]}, {row[3]}, {row[4]}')
        else:
            p = lc-1
            time.append(float(row[0]))
            temp1.append(float(row[1]))
            curr1.append(float(row[2]))
            temp2.append(float(row[3]))
            curr2.append(float(row[4]))
        lc += 1

fig, ax = plt.subplots()


ax.plot(time, temp1, label='temp 1')
ax.plot(time, curr1, label='current 1')
ax.plot(time, temp2, label='temp 2')
ax.plot(time, curr2, label='current 2')


ax.set_title('Thermostat data')
ax.set_ylabel('Temperature (°C) / Current (A)')
ax.legend()

ax.grid()