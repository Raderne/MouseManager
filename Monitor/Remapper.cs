using System.ComponentModel;
using System.Diagnostics;
using System.Runtime.InteropServices;

// Remaps Middle/Back/Forward, but only for clicks coming from the configured devices (the N230).
// Raw Input says which device a click came from; a low-level mouse hook swallows the original and runs the action instead.
sealed class Remapper : NativeWindow, IDisposable
{
    const int WH_MOUSE_LL = 14, PM_REMOVE = 1, LLMHF_INJECTED = 1, RawButtonFlagsMask = 0x3FF;
    const int WM_MBUTTONDOWN = 0x207, WM_MBUTTONUP = 0x208, WM_XBUTTONDOWN = 0x20B, WM_XBUTTONUP = 0x20C;

    readonly Native.HookProc hookProc; // must stay referenced while hooked
    readonly IntPtr hook;
    readonly Action<Exception> onError;
    readonly Queue<Action> pending = [];
    readonly Dictionary<string, ButtonAction.Handlers?> held = []; // decision made at button-down, reused at button-up
    Dictionary<string, ButtonAction.Handlers> actions = [];
    string[] devices = [];
    bool lastClickFromTarget;

    // Spike 0b (--scale): software DPI scaling. Target-device moves are swallowed in the hook and re-injected scaled from their raw counts.
    // ponytail: spike only, not used by the tray app; the Rust port decides the real design.
    const int WM_MOUSEMOVE = 0x200;
    public double Scale = 1;
    public bool Scaling; // set by --scale only; also on at x1, to measure the baseline
    bool lastMoveFromTarget;
    double statRaw, statCursor; // N230 sensor travel (counts) vs on-screen pointer travel (px)
    (int X, int Y)? lastPt;
    long accX, accY, accSince;
    double carryX, carryY;
    readonly Stopwatch clock = Stopwatch.StartNew();
    long statMoves, statSwallowed, statHookTicks, statHookMax, statFlushes, statLagTicks, statLagMax;
    TimeSpan statCpu;

    // Every raw input event (RAWINPUT bytes, device path). Raw Input allows one window per process, so the monitor listens here.
    public event Action<byte[], string>? RawInput;

    public Remapper(Action<Exception> onError)
    {
        this.onError = onError;
        CreateHandle(new CreateParams { Parent = new IntPtr(-3) }); // message-only window
        Native.RegisterRawInput(Handle, RawInputLogger.Collections);
        hookProc = OnHook;
        hook = Native.SetWindowsHookEx(WH_MOUSE_LL, hookProc, Native.GetModuleHandle(null), 0);
        if (hook == IntPtr.Zero) throw new Win32Exception();
    }

    // Throws FormatException (naming the button) if a mapping is invalid; nothing changes in that case.
    public void Apply(Config config)
    {
        var parsed = new Dictionary<string, ButtonAction.Handlers>();
        foreach (var (button, text) in config.Buttons)
        {
            try { if (ButtonAction.Parse(text) is { } h) parsed[button] = h; }
            catch (FormatException ex) { throw new FormatException($"{button}: {ex.Message}"); }
        }
        actions = parsed;
        devices = config.Devices.Where(d => d.Trim() != "").ToArray();
    }

    IntPtr OnHook(int code, IntPtr wParam, IntPtr lParam)
    {
        if (code >= 0 && Scaling && (int)wParam == WM_MOUSEMOVE)
        {
            long t0 = clock.ElapsedTicks;
            var info = Marshal.PtrToStructure<Native.MSLLHOOKSTRUCT>(lParam);
            bool swallow = false;
            if (Scale != 1 && (info.Flags & LLMHF_INJECTED) == 0)
            {
                DrainRawInput(); // learn which device this move came from
                swallow = lastMoveFromTarget;
            }
            long dt = clock.ElapsedTicks - t0;
            statMoves++; statHookTicks += dt; statHookMax = Math.Max(statHookMax, dt);
            if (swallow) { statSwallowed++; return 1; }
            if (lastPt is var (px, py)) statCursor += Math.Sqrt((double)(info.X - px) * (info.X - px) + (double)(info.Y - py) * (info.Y - py));
            lastPt = (info.X, info.Y);
            return Native.CallNextHookEx(hook, code, wParam, lParam);
        }
        if (code >= 0 && ButtonOf((int)wParam, lParam) is { } click)
        {
            if (click.Down)
            {
                DrainRawInput(); // this click's WM_INPUT is already queued; read it to learn which device sent it
                held[click.Button] = lastClickFromTarget ? actions.GetValueOrDefault(click.Button) : null;
            }
            if (held.GetValueOrDefault(click.Button) is { } h)
            {
                if (!click.Down) held[click.Button] = null;
                Post(click.Down ? h.Down : h.Up); // never SendInput inside the hook itself
                return 1; // swallow the original
            }
        }
        return Native.CallNextHookEx(hook, code, wParam, lParam);
    }

    static (string Button, bool Down)? ButtonOf(int msg, IntPtr lParam)
    {
        if (msg is not (WM_MBUTTONDOWN or WM_MBUTTONUP or WM_XBUTTONDOWN or WM_XBUTTONUP)) return null;
        var info = Marshal.PtrToStructure<Native.MSLLHOOKSTRUCT>(lParam);
        if ((info.Flags & LLMHF_INJECTED) != 0) return null; // synthetic input, including our own
        bool down = msg is WM_MBUTTONDOWN or WM_XBUTTONDOWN;
        if (msg is WM_MBUTTONDOWN or WM_MBUTTONUP) return ("Middle", down);
        return ((info.MouseData >> 16) == 1 ? "Back" : "Forward", down);
    }

    void DrainRawInput()
    {
        while (Native.PeekMessage(out var m, Handle, Native.WM_INPUT, Native.WM_INPUT, PM_REMOVE))
        {
            var msg = Message.Create(Handle, Native.WM_INPUT, m.WParam, m.LParam);
            WndProc(ref msg);
        }
    }

    protected override void WndProc(ref Message m)
    {
        if (m.Msg == Native.WM_INPUT) OnRawInput(m.LParam);
        else if (m.Msg == Native.WM_APP) RunPending();
        base.WndProc(ref m);
    }

    void OnRawInput(IntPtr hRaw)
    {
        var raw = Native.ReadRawInput(hRaw);
        int type = Native.RawType(raw);
        if (type < 0) return;
        var device = Native.RawDevice(raw);
        var name = Native.DeviceName(device);
        RawInput?.Invoke(raw, name);
        int h = Native.RawHeaderSize;
        if (type == Native.RIM_TYPEMOUSE && device != IntPtr.Zero && (BitConverter.ToUInt16(raw, h) & 1) == 0) // relative motion
        {
            int dx = BitConverter.ToInt32(raw, h + 12), dy = BitConverter.ToInt32(raw, h + 16);
            if (dx != 0 || dy != 0)
            {
                lastMoveFromTarget = IsTarget(name);
                if (Scaling && lastMoveFromTarget) statRaw += Math.Sqrt((double)dx * dx + (double)dy * dy);
                if (Scaling && Scale != 1 && lastMoveFromTarget)
                {
                    if (accX == 0 && accY == 0) accSince = clock.ElapsedTicks;
                    accX += dx; accY += dy;
                    Post(FlushMove);
                }
            }
        }
        if (type != Native.RIM_TYPEMOUSE || device == IntPtr.Zero || (BitConverter.ToUInt16(raw, Native.RawHeaderSize + 4) & RawButtonFlagsMask) == 0) return; // injected, or no button change
        lastClickFromTarget = IsTarget(name);
    }

    void FlushMove()
    {
        if (accX == 0 && accY == 0) return;
        double x = accX * Scale + carryX, y = accY * Scale + carryY; // carry the fraction so slow moves aren't lost
        int ix = (int)Math.Truncate(x), iy = (int)Math.Truncate(y);
        carryX = x - ix; carryY = y - iy;
        long lag = clock.ElapsedTicks - accSince;
        statFlushes++; statLagTicks += lag; statLagMax = Math.Max(statLagMax, lag);
        accX = accY = 0;
        if (ix != 0 || iy != 0) Native.SendMove(ix, iy);
    }

    // One line of spike stats since the last call, then resets them.
    public string TakeStats(double seconds)
    {
        double us = 1e6 / Stopwatch.Frequency;
        var cpu = Process.GetCurrentProcess().TotalProcessorTime;
        string s = $"moves {statMoves,5} swallowed {statSwallowed,5} | hook avg {(statMoves == 0 ? 0 : statHookTicks * us / statMoves),4:0} us max {statHookMax * us,5:0} us" +
                   $" | inject lag avg {(statFlushes == 0 ? 0 : statLagTicks * us / statFlushes),4:0} us max {statLagMax * us,5:0} us | CPU {(cpu - statCpu).TotalMilliseconds / (seconds * 10 * Environment.ProcessorCount),4:0.0}%" +
                   $" | pointer px / sensor counts {(statRaw == 0 ? 0 : statCursor / statRaw):0.00}";
        statMoves = statSwallowed = statHookTicks = statHookMax = statFlushes = statLagTicks = statLagMax = 0;
        statRaw = statCursor = 0;
        statCpu = cpu;
        return s;
    }

    public bool IsTarget(string devicePath) => devices.Any(d => devicePath.Contains(d, StringComparison.OrdinalIgnoreCase));

    void Post(Action action)
    {
        pending.Enqueue(action);
        Native.PostMessage(Handle, Native.WM_APP, IntPtr.Zero, IntPtr.Zero);
    }

    void RunPending()
    {
        while (pending.TryDequeue(out var action))
            try { action(); } catch (Exception ex) { onError(ex); } // a failing action (e.g. bad run: path) must not kill the app
    }

    public void Dispose()
    {
        Native.UnhookWindowsHookEx(hook);
        DestroyHandle();
    }
}
