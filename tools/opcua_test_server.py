"""Server OPC UA di test (asyncua) per il driver OPC UA.
Uso: python tools/opcua_test_server.py [porta]   (default 4841)
Nodi: ns=2;s=WCS.ReqID (UInt16), WCS.Barcode (String), WCS.Run (Boolean),
      WCS.Temp (Float), WCS.Count (Int32) – tutti scrivibili."""
import asyncio, sys
from asyncua import Server, ua

async def main():
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 4841
    srv = Server()
    await srv.init()
    srv.set_endpoint(f"opc.tcp://0.0.0.0:{port}/")
    srv.set_security_policy([ua.SecurityPolicyType.NoSecurity])
    idx = await srv.register_namespace("urn:wcs-test")
    obj = await srv.nodes.objects.add_object(idx, "WCS")
    for name, val, vt in [("ReqID", 7, ua.VariantType.UInt16), ("Barcode", "PKG-OPC-01", ua.VariantType.String),
                          ("Run", True, ua.VariantType.Boolean), ("Temp", 21.5, ua.VariantType.Float),
                          ("Count", -42, ua.VariantType.Int32)]:
        v = await obj.add_variable(ua.NodeId(f"WCS.{name}", idx), name, ua.Variant(val, vt))
        await v.set_writable()
    print(f"OPC UA test server su porta {port}, ns={idx}", flush=True)
    async with srv:
        while True:
            await asyncio.sleep(1)

asyncio.run(main())
