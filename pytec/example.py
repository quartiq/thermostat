from pytec.client import Client

tec = Client() #(host="localhost", port=6667)
tec.set_param("s-h", 1, "t0", 20)
print(tec.get_pid())
print(tec.get_steinhart_hart())
for data in tec.report_mode():
    print(data)
