// Server Modbus TCP minimo (funzioni 1, 2, 3, 4, 5, 6, 15, 16) sopra l'immagine PLCSIM.

using System;
using System.Collections.Generic;
using System.IO;
using System.Net;
using System.Net.Sockets;
using System.Threading;

namespace Ptwcs.PlcsimBridge
{
    public sealed class ModbusServer
    {
        private const ushort RegScale = 60000, RegState = 60001, RegRun = 60002;
        private readonly TcpListener _listener;
        private readonly PlcsimImage _plc;
        private volatile bool _running;

        public ModbusServer(string bind, int port, PlcsimImage plc)
        {
            _listener = new TcpListener(IPAddress.Parse(bind), port);
            _plc = plc;
        }

        public void Start()
        {
            _running = true;
            _listener.Start();
            new Thread(Accept) { IsBackground = true, Name = "modbus-accept" }.Start();
        }

        public void Stop()
        {
            _running = false;
            _listener.Stop();
        }

        private void Accept()
        {
            while (_running)
            {
                try
                {
                    var c = _listener.AcceptTcpClient();
                    c.NoDelay = true;
                    Console.WriteLine($"[bridge] client {c.Client.RemoteEndPoint}");
                    new Thread(() => Serve(c)) { IsBackground = true }.Start();
                }
                catch (SocketException) { if (!_running) return; }
            }
        }

        private static void ReadExact(Stream s, byte[] b, int n)
        {
            int off = 0;
            while (off < n)
            {
                int r = s.Read(b, off, n - off);
                if (r <= 0) throw new EndOfStreamException();
                off += r;
            }
        }

        private void Serve(TcpClient c)
        {
            using (c)
            using (var s = c.GetStream())
            {
                var head = new byte[7];
                try
                {
                    while (_running)
                    {
                        ReadExact(s, head, 7);
                        int len = head[4] << 8 | head[5];
                        if (len < 2 || len > 260) return;
                        var pdu = new byte[len - 1];
                        ReadExact(s, pdu, pdu.Length);
                        byte[] resp;
                        try { resp = Handle(pdu); }
                        catch (ArgumentOutOfRangeException) { resp = Exception(pdu[0], 0x02); }
                        catch (InvalidOperationException) { resp = Exception(pdu[0], 0x04); }
                        catch (Exception) { resp = Exception(pdu[0], 0x04); }
                        var outb = new byte[7 + resp.Length];
                        Array.Copy(head, outb, 4);
                        outb[4] = (byte)((resp.Length + 1) >> 8);
                        outb[5] = (byte)(resp.Length + 1);
                        outb[6] = head[6];
                        Array.Copy(resp, 0, outb, 7, resp.Length);
                        s.Write(outb, 0, outb.Length);
                    }
                }
                catch (IOException) { }
                catch (EndOfStreamException) { }
            }
        }

        private static byte[] Exception(byte fc, byte code) => new[] { (byte)(fc | 0x80), code };
        private static ushort U16(byte[] b, int i) => (ushort)(b[i] << 8 | b[i + 1]);

        private byte[] Handle(byte[] p)
        {
            byte fc = p[0];
            switch (fc)
            {
                case 1: case 2:
                {
                    ushort a = U16(p, 1), n = U16(p, 3);
                    if (n == 0 || n > 2000) return Exception(fc, 0x03);
                    var bytes = new byte[(n + 7) / 8];
                    for (int k = 0; k < n; k++)
                    {
                        bool v = fc == 1 ? _plc.ReadInputBit((uint)(a + k)) : _plc.ReadOutputBit((uint)(a + k));
                        if (v) bytes[k / 8] |= (byte)(1 << (k % 8));
                    }
                    var r = new byte[2 + bytes.Length];
                    r[0] = fc; r[1] = (byte)bytes.Length;
                    Array.Copy(bytes, 0, r, 2, bytes.Length);
                    return r;
                }
                case 3: case 4:
                {
                    ushort a = U16(p, 1), n = U16(p, 3);
                    if (n == 0 || n > 125) return Exception(fc, 0x03);
                    var r = new byte[2 + 2 * n];
                    r[0] = fc; r[1] = (byte)(2 * n);
                    for (int k = 0; k < n; k++)
                    {
                        ushort addr = (ushort)(a + k);
                        ushort v = fc == 4 ? _plc.ReadOutputWord(addr)
                            : addr == RegScale ? _plc.ScaleFactorX100
                            : addr == RegState ? _plc.OperatingStateCode
                            : addr == RegRun ? (ushort)(_plc.OperatingStateCode == 4 ? 1 : 0)
                            : _plc.ReadInputWord(addr);
                        r[2 + 2 * k] = (byte)(v >> 8);
                        r[3 + 2 * k] = (byte)v;
                    }
                    return r;
                }
                case 5:
                {
                    ushort a = U16(p, 1), v = U16(p, 3);
                    _plc.WriteInputBit(a, v == 0xFF00);
                    return new[] { fc, p[1], p[2], p[3], p[4] };
                }
                case 6:
                {
                    ushort a = U16(p, 1), v = U16(p, 3);
                    WriteReg(a, v);
                    return new[] { fc, p[1], p[2], p[3], p[4] };
                }
                case 15:
                {
                    ushort a = U16(p, 1), n = U16(p, 3);
                    for (int k = 0; k < n; k++) _plc.WriteInputBit((uint)(a + k), (p[6 + k / 8] >> (k % 8) & 1) == 1);
                    return new[] { fc, p[1], p[2], p[3], p[4] };
                }
                case 16:
                {
                    ushort a = U16(p, 1), n = U16(p, 3);
                    for (int k = 0; k < n; k++) WriteReg((ushort)(a + k), U16(p, 6 + 2 * k));
                    return new[] { fc, p[1], p[2], p[3], p[4] };
                }
                default:
                    return Exception(fc, 0x01);
            }
        }

        private void WriteReg(ushort a, ushort v)
        {
            if (a == RegScale) _plc.ScaleFactorX100 = v;
            else if (a == RegRun) _plc.RunStop(v != 0);
            else if (a == RegState) throw new ArgumentOutOfRangeException(nameof(a));
            else _plc.WriteInputWord(a, v);
        }
    }
}
