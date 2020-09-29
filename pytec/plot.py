import numpy as np
import matplotlib.pyplot as plt
import matplotlib.animation as animation
from threading import Thread, Lock
from pytec.client import Client

TIME_WINDOW = 300.0

class Series:
    def __init__(self, scale=1.0):
        self.scale = scale
        self.x_data = []
        self.y_data = []

    def append(self, x, y):
        self.x_data.append(x)
        self.y_data.append(self.scale * y)

    def clip(self, min_x):
        drop = 0
        while drop < len(self.x_data) and self.x_data[drop] < min_x:
            drop += 1
        self.x_data = self.x_data[drop:]
        self.y_data = self.y_data[drop:]
        
series = {
    'adc': Series(),
    'sens': Series(0.0001),
    'temperature': Series(),
    'i_set': Series(),
    'vref': Series(),
    'dac_feedback': Series(),
    'i_tec': Series(),
    'tec_i': Series(),
    'tec_u_meas': Series(),
}
series_lock = Lock()

quit = False

def recv_data(tec):
    print("reporting")
    for data in tec.report_mode():
        if data['channel'] == 0:
            series_lock.acquire()
            try:
                time = data['time'] / 1000.0
                for k, s in series.iteritems():
                    v = data[k]
                    if data.has_key(k) and type(v) is float:
                        s.append(time, v)
            finally:
                series_lock.release()

        if quit:
            break

tec = Client()
print("connected")
thread = Thread(target=recv_data, args=(tec,))
thread.start()

fig, ax = plt.subplots()

for k, s in series.iteritems():
    s.plot, = ax.plot([], [], label=k)
ax.legend()

def animate(i):
    min_x, max_x, min_y, max_y = None, None, None, None
    
    series_lock.acquire()
    try:
        for s in series.itervalues():
            s.plot.set_data(s.x_data, s.y_data)

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
            for s in series.itervalues():
                s.clip(max_x - TIME_WINDOW)
    finally:
        series_lock.release()

    margin_y = 0.01 * (max_y - min_y)
    ax.set_xlim(min_x, max_x)
    ax.set_ylim(min_y - margin_y, max_y + margin_y)


ani = animation.FuncAnimation(
    fig, animate, interval=1, blit=False, save_count=50)

# To save the animation, use e.g.
#
# ani.save("movie.mp4")
#
# or
#
# writer = animation.FFMpegWriter(
#     fps=15, metadata=dict(artist='Me'), bitrate=1800)
# ani.save("movie.mp4", writer=writer)

print("show")
plt.show()
quit = True
thread.join()
