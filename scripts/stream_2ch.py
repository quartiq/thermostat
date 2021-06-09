import socket
import matplotlib.pyplot as plt
import matplotlib.animation as animation
import datetime as dt

s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
s.connect(('192.168.1.26', 23))

temp_offset1 = -45   # for example to display the temp error instead
temp_offset2 = -45   # for example to display the temp error instead

fig = plt.figure()
ax = fig.add_subplot(1, 1, 1)
xs = []
temps1 = []
currents1 = []
temps2 = []
currents2 = []

def animate(i, xs, temps1, currents1, temps2, currents2):
    s.send('report\n'.encode())

    msg = s.recv(1024).decode("utf-8")

    m1, m2, m3 = msg.split("\"temperature\":")
    m2, m4 = m2.split(",\"pid_engaged")
    temp1 = float(m2) + temp_offset1

    m4, m5 = m4.split("\"i_set\":")
    m5, m6 = m5.split(",\"vref\":")

    i_set1 = float(m5)
    if 10 < i_set1:
        i_set1 = 10
    if -10 > i_set1:
        i_set1 = -10

    m2, m4 = m3.split(",\"pid_engaged")
    temp2 = float(m2) + temp_offset2
    print(temp1, temp2)

    m4, m5 = m4.split("\"i_set\":")
    m5, m6 = m5.split(",\"vref\":")

    i_set2 = float(m5)
    if 10 < i_set2:
        i_set2 = 10
    if -10 > i_set2:
        i_set2 = -10

    xs.append(dt.datetime.now().strftime('%H:%M:%S.%f'))
    temps1.append(temp1)
    currents1.append(i_set1)

    temps2.append(temp2)
    currents2.append(i_set2)


     # Limit x and y lists to 100 items
    xs = xs[-200:]
    temps1 = temps1[-200:]
    currents1 = currents1[-200:]
    temps2 = temps2[-200:]
    currents2 = currents2[-200:]

    # Draw x and y lists
    ax.clear()
    ax.plot(xs, temps1, currents1)
    ax.plot(xs, temps2, currents2)
    ax.grid()

    # Format plot
    plt.xticks(rotation=45, ha='right')
    plt.subplots_adjust(bottom=0.30)
    plt.title('Thermostat data')
    plt.ylabel('Temperature (°C) / Current (A)')

# Set up plot to call animate() function periodically
ani = animation.FuncAnimation(fig, animate, fargs=(xs, temps1, currents1, temps2, currents2), interval=200)
plt.show()
