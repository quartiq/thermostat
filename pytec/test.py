from pytec.client import Client

tec = Client() #(host="localhost", port=6667)
tec.set_param("s-h", 0, "t", 20)
for data in tec.report_mode():
    print(data)
