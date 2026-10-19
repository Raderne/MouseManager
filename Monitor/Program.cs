static class Program
{
    [STAThread]
    static int Main(string[] args)
    {
        if (args is ["--log", ..]) return Log(args.Length > 1 && int.TryParse(args[1], out var s) ? s : 60);
        if (args is ["--selftest"]) return SelfTest();

        using var mutex = new Mutex(true, "MouseManager.N230", out bool first);
        if (!first) return 0; // already running; a second hook would fire every action twice
        ApplicationConfiguration.Initialize();
        Application.Run(new TrayApp());
        return 0;
    }

    // Prints what every button sends, tagged by device. Use it to find the Bluetooth/wired device IDs.
    static int Log(int seconds)
    {
        Native.AttachConsole(-1);
        using var logger = new RawInputLogger();
        var timer = new System.Windows.Forms.Timer { Interval = seconds * 1000 };
        timer.Tick += (_, _) => Application.Exit();
        timer.Start();
        Console.WriteLine($"Logging for {seconds}s - press each button a few times, slowly.");
        Application.Run();
        return 0;
    }

    static int SelfTest()
    {
        Native.AttachConsole(-1);
        static bool Throws(Action a) { try { a(); return false; } catch (FormatException) { return true; } }
        var failures = new List<string>();
        void Check(bool ok, string what) { if (!ok) failures.Add(what); }

        Check(ButtonAction.Parse("Default") is null && ButtonAction.Parse(" ") is null, "Default passes through");
        Check(ButtonAction.Parse("disabled") is not null && ButtonAction.Parse("back") is not null, "Disabled / mouse button");
        Check(ButtonAction.ParseKey("ctrl") == Keys.ControlKey && ButtonAction.ParseKey("Win") == Keys.LWin && ButtonAction.ParseKey("alt") == Keys.Menu, "modifiers");
        Check(ButtonAction.ParseKey("5") == Keys.D5 && ButtonAction.ParseKey("f5") == Keys.F5 && ButtonAction.ParseKey("Backspace") == Keys.Back, "digits, F-keys, aliases");
        Check(ButtonAction.Parse("Ctrl+Shift+T") is not null && ButtonAction.Parse("VolumeUp") is not null && ButtonAction.Parse("run:calc.exe") is not null, "combos / run");
        foreach (var bad in new[] { "Ctrl+Nope", "Ctrl+", "run:", "Ctrl,Shift", "Alt+65536" })
            Check(Throws(() => ButtonAction.Parse(bad)), $"rejects '{bad}'");
        Check(Native.InputSize == (IntPtr.Size == 8 ? 40 : 28), "INPUT struct size");
        Check(Math.Abs(MonitorForm.Dpi(3150, 10) - 800.1) < 0.1, "DPI from counts over 10 cm");

        var raw = new byte[Native.RawHeaderSize + 24]; // RIM_TYPEMOUSE
        Check(RawInputLogger.Describe(raw) is null, "pure movement is not described");
        raw[Native.RawHeaderSize + 4] = 0x40; // RI_MOUSE_BUTTON_4_DOWN
        Check(RawInputLogger.Describe(raw)?.StartsWith("MOUSE X1-down") == true, "X1 decoded");

        // Informational (needs the mouse connected): can we reach the settings report?
        using (var s = Native.OpenFeatureCollection(p => p.Contains(RawInputLogger.N230, StringComparison.OrdinalIgnoreCase))?.Handle)
            Console.WriteLine("settings report: " + (s is null ? "collection not found" :
                Native.GetFeature(s, 6, 8) is { } r ? Convert.ToHexString(r) : "read failed"));

        Console.WriteLine(failures.Count == 0 ? "selftest OK": "FAILED:\n  " + string.Join("\n  ", failures));
        return failures.Count == 0 ? 0 : 1;
    }
}
