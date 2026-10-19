using System.ComponentModel;
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
        if (type != Native.RIM_TYPEMOUSE || device == IntPtr.Zero || (BitConverter.ToUInt16(raw, Native.RawHeaderSize + 4) & RawButtonFlagsMask) == 0) return; // injected, or no button change
        lastClickFromTarget = IsTarget(name);
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
