import numpy as np
import matplotlib.pyplot as plt
import matplotlib.animation as animation
from threading import Thread, Lock
from pytec.client import Client

TIME_WINDOW = 300.0

tec = Client()
target_temperature = tec.get_pid()[0]['target']
print("Channel 0 target temperature: {:.3f}".format(target_temperature))

class Series:
    def __init__(self, conv=lambda x: x):
        self.conv = conv
        self.x_data = []
        self.y_data = []

    def append(self, x, y):
        self.x_data.append(x)
        self.y_data.append(self.conv(y))

    def clip(self, min_x):
        drop = 0
        while drop < len(self.x_data) and self.x_data[drop] < min_x:
            drop += 1
        self.x_data = self.x_data[drop:]
        self.y_data = self.y_data[drop:]
        
series = {
    'adc': Series(),
    'sens': Series(lambda x: x * 0.0001),
    'temperature': Series(lambda t: t - target_temperature),
    'i_set': Series(),
    'pid_output': Series(),
    'vref': Series(),
    'dac_value': Series(),
    'dac_feedback': Series(),
    'i_tec': Series(),
    'tec_i': Series(),
    'tec_u_meas': Series(),
    'interval': Series(),
}
series_lock = Lock()

quit = False
last_packet_time = None

def recv_data(tec):
    global last_packet_time
    for data in tec.report_mode():
        if data['channel'] == 0:
            series_lock.acquire()
            try:
                time = data['time'] / 1000.0
                if last_packet_time:
                    data['interval'] = time - last_packet_time
                last_packet_time = time

                for k, s in series.items():
                    if k in data:
                        v = data[k]
                        if type(v) is float:
                            s.append(time, v)
            finally:
                series_lock.release()

        if quit:
            break

thread = Thread(target=recv_data, args=(tec,))
thread.start()

fig, ax = plt.subplots()

for k, s in series.items():
    s.plot, = ax.plot([], [], label=k)
legend = ax.legend()

def animate(i):
    min_x, max_x, min_y, max_y = None, None, None, None
    
    series_lock.acquire()
    try:
        for k, s in series.items():
            s.plot.set_data(s.x_data, s.y_data)
            if len(s.y_data) > 0:
                s.plot.set_label("{}: {:.3f}".format(k, s.y_data[-1]))

            if len(s.x_data) > 0:
                min_x_ = min(s.x_data)
                if min_x is None:
                    min_x = min_x_
                else:
                    min_x = min(min_x, min_x_)
                max_x_ = max(s.x_data)
                if max_x is None:
                    max_x = max_x_
                else:
                    max_x = max(max_x, max_x_)
            if len(s.y_data) > 0:
                min_y_ = min(s.y_data)
                if min_y is None:
                    min_y = min_y_
                else:
                    min_y = min(min_y, min_y_)
                max_y_ = max(s.y_data)
                if max_y is None:
                    max_y = max_y_
                else:
                    max_y = max(max_y, max_y_)

        if min_x is not None and max_x - TIME_WINDOW > min_x:
            for s in series.values():
                s.clip(max_x - TIME_WINDOW)
    finally:
        series_lock.release()

    if min_x != max_x:
        ax.set_xlim(min_x, max_x)
    if min_y != max_y:
        margin_y = 0.01 * (max_y - min_y)
        ax.set_ylim(min_y - margin_y, max_y + margin_y)

    global legend
    legend.remove()
    legend = ax.legend()

ani = animation.FuncAnimation(
    fig, animate, interval=1, blit=False, save_count=50)

plt.show()
quit = True
thread.join()
