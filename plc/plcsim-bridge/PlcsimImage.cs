// Accesso all'immagine di processo dell'istanza PLCSIM Advanced (API Siemens.Simatic.Simulation.Runtime).
// Tutte le chiamate all'API passano da un lock: il server Modbus può servire più client.

using System;
using System.Threading;
using Siemens.Simatic.Simulation.Runtime;

namespace Ptwcs.PlcsimBridge
{
    public sealed class PlcsimImage
    {
        private readonly BridgeConfig _cfg;
        private readonly object _lock = new object();
        private IInstance _inst;

        public PlcsimImage(BridgeConfig cfg) { _cfg = cfg; }

        public bool Connected { get { lock (_lock) return _inst != null; } }

        /// <summary>Collega (e ricollega) l'istanza; opzionalmente la accende e la mette in RUN.</summary>
        public void KeepConnected()
        {
            while (true)
            {
                if (!Connected)
                {
                    try
                    {
                        var inst = SimulationRuntimeManager.CreateInterface(_cfg.instance);
                        if (_cfg.powerOnAndRun)
                        {
                            if (inst.OperatingState == EOperatingState.Off) inst.PowerOn(_cfg.connectTimeoutMs);
                            if (inst.OperatingState != EOperatingState.Run) inst.Run(_cfg.connectTimeoutMs);
                        }
                        lock (_lock) _inst = inst;
                        Console.WriteLine($"[bridge] collegato a '{_cfg.instance}' ({inst.OperatingState})");
                    }
                    catch (Exception e)
                    {
                        Console.WriteLine($"[bridge] istanza '{_cfg.instance}' non disponibile: {e.Message}");
                    }
                }
                Thread.Sleep(2000);
            }
        }

        private T Use<T>(Func<IInstance, T> f)
        {
            lock (_lock)
            {
                if (_inst == null) throw new InvalidOperationException("PLCSIM non collegato");
                try { return f(_inst); }
                catch (Exception e)
                {
                    Console.WriteLine($"[bridge] errore API: {e.Message}: ricollegamento");
                    _inst = null;
                    throw;
                }
            }
        }

        private void Check(uint byteAddr, uint size, string area)
        {
            if (byteAddr >= size) throw new ArgumentOutOfRangeException(nameof(byteAddr), $"{area}B{byteAddr} oltre l'area configurata ({size} byte)");
        }

        // ---------------------------------------------------------------- bit
        public bool ReadInputBit(uint n) { Check(n / 8, _cfg.inputBytes, "%I"); return Use(i => i.InputArea.ReadBit(n / 8, (byte)(n % 8))); }
        public void WriteInputBit(uint n, bool v) { Check(n / 8, _cfg.inputBytes, "%I"); Use(i => { i.InputArea.WriteBit(n / 8, (byte)(n % 8), v); return 0; }); }
        public bool ReadOutputBit(uint n) { Check(n / 8, _cfg.outputBytes, "%Q"); return Use(i => i.OutputArea.ReadBit(n / 8, (byte)(n % 8))); }

        // ---------------------------------------------------------------- word (big-endian, come S7)
        public ushort ReadInputWord(uint n)
        {
            Check(2 * n + 1, _cfg.inputBytes, "%I");
            return Use(i => (ushort)(i.InputArea.ReadByte(2 * n) << 8 | i.InputArea.ReadByte(2 * n + 1)));
        }
        public void WriteInputWord(uint n, ushort v)
        {
            Check(2 * n + 1, _cfg.inputBytes, "%I");
            Use(i => { i.InputArea.WriteByte(2 * n, (byte)(v >> 8)); i.InputArea.WriteByte(2 * n + 1, (byte)v); return 0; });
        }
        public ushort ReadOutputWord(uint n)
        {
            Check(2 * n + 1, _cfg.outputBytes, "%Q");
            return Use(i => (ushort)(i.OutputArea.ReadByte(2 * n) << 8 | i.OutputArea.ReadByte(2 * n + 1)));
        }

        // ---------------------------------------------------------------- registri speciali
        public ushort ScaleFactorX100
        {
            get => Use(i => (ushort)Math.Round(i.ScaleFactor * 100.0));
            set => Use(i => { i.ScaleFactor = Math.Max(0.01, value / 100.0); return 0; });
        }

        public ushort OperatingStateCode => Use(i =>
        {
            switch (i.OperatingState)
            {
                case EOperatingState.Off: return (ushort)1;
                case EOperatingState.Stop: return (ushort)2;
                case EOperatingState.Booting:
                case EOperatingState.Startup: return (ushort)3;
                case EOperatingState.Run: return (ushort)4;
                default: return (ushort)0;
            }
        });

        public void RunStop(bool run) => Use(i => { if (run) i.Run(_cfg.connectTimeoutMs); else i.Stop(_cfg.connectTimeoutMs); return 0; });
    }
}
