"""Server S7 di test (python-snap7) per i test di integrazione del driver S7.
Espone DB10 (256 byte) e area M (64 byte) con valori noti.
Uso: python tools/s7_test_server.py [porta]   (default 1102)"""
import ctypes, struct, sys, time
from snap7.server import Server
from snap7.type import SrvArea

port = int(sys.argv[1]) if len(sys.argv) > 1 else 1102
db10 = (ctypes.c_ubyte * 256)()
mk = (ctypes.c_ubyte * 64)()
db10[0] = 0b0000_1010                       # DBX0.1 = DBX0.3 = 1
struct.pack_into(">h", db10, 2, -1234)      # DBW2
struct.pack_into(">f", db10, 4, 12.5)       # DBD4 real
struct.pack_into(">I", db10, 8, 3000000000) # DBD8 udint
db10[20], db10[21] = 20, 6                  # STRING[20] @20
db10[22:28] = list(b"PKG001")
struct.pack_into(">H", mk, 10, 4242)        # MW10
srv = Server()
srv.register_area(SrvArea.DB, 10, db10)
srv.register_area(SrvArea.MK, 0, mk)
srv.start(tcp_port=port)
print(f"S7 test server su porta {port}", flush=True)
try:
    while True:
        time.sleep(1)
except KeyboardInterrupt:
    srv.stop()
