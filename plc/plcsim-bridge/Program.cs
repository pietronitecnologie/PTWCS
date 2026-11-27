// PTWCS – Ponte PLCSIM Advanced
//
// Espone l'immagine di processo di un'istanza PLCSIM Advanced come server Modbus TCP, così il
// layer fisico di PTWCS (driver modbus) scrive DIRETTAMENTE gli ingressi %I e legge le uscite %Q:
// il programma PLC resta identico a quello di produzione (nessun DB "SimIO" da aggiungere).
//
//   Modbus                     PLCSIM Advanced
//   coil N        (r/w)   ↔    %I (N/8).(N%8)         sensori bool scritti dalla fisica
//   discrete in N (r)     ↔    %Q (N/8).(N%8)         attuatori bool comandati dal PLC
//   holding reg N (r/w)   ↔    %IW(2·N)  (= %IB 2N, 2N+1, big-endian come S7)
//   input reg N   (r)     ↔    %QW(2·N)
//   holding 60000 (r/w)        fattore tempo × 100 (ScaleFactor dell'istanza: 100 = tempo reale)
//   holding 60001 (r)          stato operativo (0 sconosciuto, 1 off, 2 stop, 3 avvio, 4 run, 9 errore)
//   holding 60002 (w)          1 = RUN, 0 = STOP
//
// Con la sincronizzazione del tempo (holding 60000 scritto da PTWCS: Designer → ⚛ Fisica →
// "Tag fattore tempo") il PLC virtuale e la fisica accelerano o rallentano insieme.
//
// Uso:  PlcsimBridge.exe [bridge.json]
// Requisiti: Windows x64, PLCSIM Advanced (V4 o successiva) con l'istanza già creata
// (PLCSIM Advanced Control Panel o API) e il programma caricato da TIA Portal.

using System;
using System.IO;
using System.Text.Json;
using System.Threading;

namespace Ptwcs.PlcsimBridge
{
    public sealed class BridgeConfig
    {
        public string instance { get; set; } = "PLC_1";
        public string bind { get; set; } = "0.0.0.0";
        public int port { get; set; } = 5030;
        public uint connectTimeoutMs { get; set; } = 60000;
        public bool powerOnAndRun { get; set; } = true;
        public uint inputBytes { get; set; } = 1024;
        public uint outputBytes { get; set; } = 1024;
    }

    internal static class Program
    {
        private static int Main(string[] args)
        {
            var path = args.Length > 0 ? args[0] : Path.Combine(AppDomain.CurrentDomain.BaseDirectory, "bridge.json");
            BridgeConfig cfg;
            try
            {
                cfg = File.Exists(path) ? JsonSerializer.Deserialize<BridgeConfig>(File.ReadAllText(path)) : new BridgeConfig();
            }
            catch (Exception e)
            {
                Console.Error.WriteLine($"config {path}: {e.Message}");
                return 2;
            }
            Console.WriteLine($"[bridge] istanza PLCSIM Advanced '{cfg.instance}' → Modbus TCP {cfg.bind}:{cfg.port}");
            var plc = new PlcsimImage(cfg);
            var server = new ModbusServer(cfg.bind, cfg.port, plc);
            var stop = new ManualResetEvent(false);
            Console.CancelKeyPress += (_, e) => { e.Cancel = true; stop.Set(); };
            new Thread(plc.KeepConnected) { IsBackground = true, Name = "plcsim" }.Start();
            server.Start();
            Console.WriteLine("[bridge] in ascolto (Ctrl+C per uscire)");
            stop.WaitOne();
            server.Stop();
            return 0;
        }
    }
}
