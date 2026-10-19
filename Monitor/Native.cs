using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Text;
using Microsoft.Win32.SafeHandles;

// Win32 interop: Raw Input, low-level hooks and SendInput.
static class Native
{
    public const int WM_INPUT = 0x00FF, WM_APP = 0x8000, RIDEV_INPUTSINK = 0x100, RIDEV_PAGEONLY = 0x20, RIM_TYPEMOUSE = 0, RIM_TYPEHID = 2;
    const int RID_INPUT = 0x10000003, RIDI_DEVICENAME = 0x20000007;
    public static readonly int RawHeaderSize = 8 + 2 * IntPtr.Size;

    [StructLayout(LayoutKind.Sequential)] struct RAWINPUTDEVICE { public ushort Page, Usage; public int Flags; public IntPtr Target; }
    [StructLayout(LayoutKind.Sequential)] struct MOUSEINPUT { public int Dx, Dy, MouseData, Flags, Time; public IntPtr Extra; }
    [StructLayout(LayoutKind.Sequential)] struct KEYBDINPUT { public ushort Vk, Scan; public int Flags, Time; public IntPtr Extra; }
    [StructLayout(LayoutKind.Explicit)] struct INPUTUNION { [FieldOffset(0)] public MOUSEINPUT Mi; [FieldOffset(0)] public KEYBDINPUT Ki; }
    [StructLayout(LayoutKind.Sequential)] struct INPUT { public int Type; public INPUTUNION U; }
    [StructLayout(LayoutKind.Sequential)] public struct MSLLHOOKSTRUCT { public int X, Y, MouseData, Flags, Time; public IntPtr Extra; }
    [StructLayout(LayoutKind.Sequential)] public struct MSG { public IntPtr Hwnd; public int Message; public IntPtr WParam, LParam; public int Time, X, Y, Private; }
    public delegate IntPtr HookProc(int code, IntPtr wParam, IntPtr lParam);

    [DllImport("user32.dll", SetLastError = true)] static extern bool RegisterRawInputDevices(RAWINPUTDEVICE[] d, int n, int size);
    [DllImport("user32.dll")] static extern int GetRawInputData(IntPtr h, int cmd, byte[]? data, ref int size, int headerSize);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetRawInputDeviceInfo(IntPtr h, int cmd, StringBuilder? data, ref int size);
    [DllImport("user32.dll")] static extern uint SendInput(int n, INPUT[] inputs, int size);
    [DllImport("user32.dll")] static extern uint MapVirtualKey(uint code, uint type);
    [DllImport("user32.dll", SetLastError = true)] public static extern IntPtr SetWindowsHookEx(int id, HookProc proc, IntPtr hMod, int thread);
    [DllImport("user32.dll")] public static extern bool UnhookWindowsHookEx(IntPtr hook);
    [DllImport("user32.dll")] public static extern IntPtr CallNextHookEx(IntPtr hook, int code, IntPtr wParam, IntPtr lParam);
    [DllImport("user32.dll")] public static extern bool PeekMessage(out MSG msg, IntPtr hwnd, int min, int max, int remove);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr hwnd, int msg, IntPtr wParam, IntPtr lParam);
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode)] public static extern IntPtr GetModuleHandle(string? name);
    [DllImport("kernel32.dll")] public static extern bool AttachConsole(int pid);
    [DllImport("kernel32.dll", SetLastError = true, CharSet = CharSet.Unicode)] static extern SafeFileHandle CreateFile(string name, uint access, uint share, IntPtr sa, uint mode, uint flags, IntPtr template);
    [DllImport("hid.dll")] static extern bool HidD_GetPreparsedData(SafeFileHandle h, out IntPtr data);
    [DllImport("hid.dll")] static extern bool HidD_FreePreparsedData(IntPtr data);
    [DllImport("hid.dll")] static extern int HidP_GetCaps(IntPtr data, short[] caps);
    [DllImport("hid.dll")] static extern bool HidD_GetFeature(SafeFileHandle h, byte[] buf, int len);
    [DllImport("cfgmgr32.dll", CharSet = CharSet.Unicode)] static extern int CM_Get_Device_Interface_List_Size(out int len, ref Guid cls, string? id, int flags);
    [DllImport("cfgmgr32.dll", CharSet = CharSet.Unicode)] static extern int CM_Get_Device_Interface_List(ref Guid cls, string? id, char[] buf, int len, int flags);

    static readonly Dictionary<IntPtr, string> deviceNames = [];

    public static void RegisterRawInput(IntPtr hwnd, params (ushort Page, ushort Usage, int Flags)[] devices)
    {
        var d = devices.Select(x => new RAWINPUTDEVICE { Page = x.Page, Usage = x.Usage, Flags = x.Flags | RIDEV_INPUTSINK, Target = hwnd }).ToArray();
        if (!RegisterRawInputDevices(d, d.Length, Marshal.SizeOf<RAWINPUTDEVICE>())) throw new Win32Exception();
    }

    // WM_INPUT lParam -> RAWINPUT bytes (header + payload); empty on failure.
    public static byte[] ReadRawInput(IntPtr hRaw)
    {
        int size = 0;
        GetRawInputData(hRaw, RID_INPUT, null, ref size, RawHeaderSize);
        var buf = new byte[size];
        return GetRawInputData(hRaw, RID_INPUT, buf, ref size, RawHeaderSize) == size ? buf : [];
    }

    public static int RawType(byte[] raw) => raw.Length >= RawHeaderSize ? BitConverter.ToInt32(raw, 0) : -1;
    public static IntPtr RawDevice(byte[] raw) => IntPtr.Size == 8 ? (IntPtr)BitConverter.ToInt64(raw, 8) : BitConverter.ToInt32(raw, 8);

    public static string DeviceName(IntPtr device)
    {
        if (device == IntPtr.Zero) return ""; // injected input
        if (deviceNames.TryGetValue(device, out var name)) return name;
        int len = 0;
        GetRawInputDeviceInfo(device, RIDI_DEVICENAME, null, ref len);
        var sb = new StringBuilder(len + 1);
        GetRawInputDeviceInfo(device, RIDI_DEVICENAME, sb, ref len);
        return deviceNames[device] = sb.ToString();
    }

    public static string[] HidInterfacePaths()
    {
        var hid = new Guid("4d1e55b2-f16f-11cf-88cb-001111000030");
        if (CM_Get_Device_Interface_List_Size(out int len, ref hid, null, 0) != 0) return [];
        var buf = new char[len];
        if (CM_Get_Device_Interface_List(ref hid, null, buf, len, 0) != 0) return [];
        return new string(buf).Split('\0', StringSplitOptions.RemoveEmptyEntries);
    }

    // Opens (without read/write access) the first matching HID collection that has feature reports: the mouse's settings channel.
    public static (SafeFileHandle Handle, int Length)? OpenFeatureCollection(Func<string, bool> match)
    {
        foreach (var path in HidInterfacePaths().Where(match))
        {
            var h = CreateFile(path, 0, 3, IntPtr.Zero, 3, 0, IntPtr.Zero);
            if (h.IsInvalid) continue;
            int len = 0;
            if (HidD_GetPreparsedData(h, out var pp))
            {
                var caps = new short[32];
                if (HidP_GetCaps(pp, caps) == 0x00110000) len = caps[4]; // FeatureReportByteLength
                HidD_FreePreparsedData(pp);
            }
            if (len > 0) return (h, len);
            h.Dispose();
        }
        return null;
    }

    public static byte[]? GetFeature(SafeFileHandle h, byte reportId, int len)
    {
        var buf = new byte[len];
        buf[0] = reportId;
        return HidD_GetFeature(h, buf, len) ? buf : null;
    }

    public static int InputSize => Marshal.SizeOf<INPUT>();

    public static void SendKey(Keys key, bool down)
    {
        var ki = new KEYBDINPUT { Vk = (ushort)key, Scan = (ushort)MapVirtualKey((uint)key, 0), Flags = (down ? 0 : 2) | (IsExtended(key) ? 1 : 0) };
        Send(new INPUT { Type = 1, U = new INPUTUNION { Ki = ki } });
    }

    // Indexed like ButtonAction.MouseButtons: Left, Right, Middle, Back (X1), Forward (X2).
    static readonly (int Down, int Up, int Data)[] MouseFlags = [(0x2, 0x4, 0), (0x8, 0x10, 0), (0x20, 0x40, 0), (0x80, 0x100, 1), (0x80, 0x100, 2)];

    public static void SendMouseButton(int button, bool down)
    {
        var (d, u, data) = MouseFlags[button];
        Send(new INPUT { Type = 0, U = new INPUTUNION { Mi = new MOUSEINPUT { Flags = down ? d : u, MouseData = data } } });
    }

    static void Send(INPUT input) => SendInput(1, [input], InputSize);

    static bool IsExtended(Keys k) => k is >= Keys.PageUp and <= Keys.Down or Keys.Insert or Keys.Delete or Keys.LWin or Keys.RWin
        or Keys.Apps or Keys.Divide or Keys.NumLock or Keys.RControlKey or Keys.RMenu or >= Keys.BrowserBack and <= Keys.LaunchApplication2;
}
