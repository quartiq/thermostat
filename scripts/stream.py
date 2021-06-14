import socket
import matplotlib.pyplot as plt
import matplotlib.animation as animation
import datetime as dt

s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
s.connect(('192.168.1.26', 23))

temp_offset = -45   # for example to display the temp error instead

fig = plt.figure()
ax = fig.add_subplot(1, 1, 1)
xs = []
temps1 = []
currents1 = []

def animate(i, xs, temps1, currents1):
    s.send('report\n'.encode())

    msg = s.recv(1024).decode("utf-8")

    m1, m2, m3 = msg.split("\"temperature\":")
    m2, m4 = m2.split(",\"pid_engaged")
    temp1 = float(m2) + temp_offset
    print(temp1)

    m4, m5 = m4.split("\"i_set\":")
    m5, m6 = m5.split(",\"vref\":")

    i_set1 = float(m5)
    if 10 < i_set1:
        i_set1 = 10
    if -10 > i_set1:
        i_set1 = -10

    xs.append(dt.datetime.now().strftime('%H:%M:%S.%f'))
    temps1.append(temp1)
    currents1.append(i_set1)
     # Limit x and y lists to 100 items
    xs = xs[-50:]
    temps1 = temps1[-50:]
    currents1 = currents1[-50:]

    # Draw x and y lists
    ax.clear()
    ax.plot(xs, temps1, currents1)
    ax.grid()

    # Format plot
    plt.xticks(rotation=45, ha='right')
    plt.subplots_adjust(bottom=0.30)
    plt.title('Thermostat data')
    plt.ylabel('Temperature (°C) / Current (A)')

# Set up plot to call animate() function periodically
ani = animation.FuncAnimation(fig, animate, fargs=(xs, temps1, currents1), interval=200)
plt.show()
