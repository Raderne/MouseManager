// Diagnostic mode (--log): prints every button press and every report on the mouse's extra HID channels, tagged by device.
sealed class RawInputLogger : NativeWindow, IDisposable
{
    public const string N230 = "VID_3554&PID_FC01";

    // Mouse, system control, consumer (media keys) and the N230's two vendor input pages.
    public static readonly (ushort Page, ushort Usage, int Flags)[] Collections =
        [(0x01, 0x02, 0), (0x01, 0x80, 0), (0x0C, 0, Native.RIDEV_PAGEONLY), (0xFF01, 0, Native.RIDEV_PAGEONLY), (0xFFBC, 0, Native.RIDEV_PAGEONLY)];

    static readonly string[] FlagNames = ["L-down", "L-up", "R-down", "R-up", "M-down", "M-up", "X1-down", "X1-up", "X2-down", "X2-up", "Wheel", "HWheel"];

    public RawInputLogger()
    {
        CreateHandle(new CreateParams { Parent = new IntPtr(-3) }); // message-only window
        Native.RegisterRawInput(Handle, Collections);
    }

    protected override void WndProc(ref Message m)
    {
        if (m.Msg == Native.WM_INPUT)
        {
            var raw = Native.ReadRawInput(m.LParam);
            if (Describe(raw) is { } text)
            {
                string name = Native.DeviceName(Native.RawDevice(raw));
                string tag = name.Contains(N230, StringComparison.OrdinalIgnoreCase) ? "N230 " : "other";
                Console.WriteLine($"{DateTime.Now:HH:mm:ss.fff} [{tag}] {text}  {name}");
            }
        }
        base.WndProc(ref m);
    }

    // Human-readable RAWINPUT; null for pure mouse movement (and anything unreadable).
    public static string? Describe(byte[] raw)
    {
        int type = Native.RawType(raw), h = Native.RawHeaderSize;
        if (type == Native.RIM_TYPEMOUSE)
        {
            ushort flags = BitConverter.ToUInt16(raw, h + 4);
            if (flags == 0) return null;
            var decoded = string.Join(" ", FlagNames.Where((_, i) => (flags & (1 << i)) != 0));
            return $"MOUSE {decoded} data={BitConverter.ToInt16(raw, h + 6)}";
        }
        if (type == Native.RIM_TYPEHID)
        {
            int sizeHid = BitConverter.ToInt32(raw, h), count = BitConverter.ToInt32(raw, h + 4);
            return $"HID {Convert.ToHexString(raw, h + 8, sizeHid * count)}";
        }
        return null;
    }

    public void Dispose() => DestroyHandle();
}
