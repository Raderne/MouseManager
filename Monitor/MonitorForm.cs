using Microsoft.Win32.SafeHandles;

// Live view of what the N230 sends, plus a DPI meter.
// The meter sums raw sensor counts per slide; raw counts ignore Windows pointer speed/acceleration,
// so counts / inches is the real DPI and a DPI-button press shows up as a jump between slides.
sealed class MonitorForm : Form
{
    const byte SettingsReportId = 6; // the only feature report the N230 answers
    const int StrokeEndMs = 400, MinStrokeCounts = 20, MaxLines = 1000;
    const double DpiJump = 0.15;

    readonly Remapper remapper;
    readonly ListBox events = new() { Dock = DockStyle.Fill, IntegralHeight = false, HorizontalScrollbar = true, Font = new Font(FontFamily.GenericMonospace, 9f) };
    readonly CheckBox allDevices = new() { Text = "Show all devices (to find Bluetooth / wired IDs)", AutoSize = true, Margin = new Padding(12, 6, 3, 3) };
    readonly CheckBox measure = new() { Text = "Measure DPI - slide the mouse in a straight line over", AutoSize = true, Margin = new Padding(3, 6, 0, 3) };
    readonly NumericUpDown distance = new() { Minimum = 1, Maximum = 100, Value = 10, DecimalPlaces = 1, Width = 60 };
    readonly Label dpiLabel = new() { AutoSize = true, Text = "cm", Margin = new Padding(3, 6, 3, 3) };
    readonly Label settingsLabel = new() { AutoSize = true, Margin = new Padding(3, 6, 3, 3) };
    readonly System.Windows.Forms.Timer timer = new() { Interval = 100 };
    readonly List<string> buffered = []; // raw input can arrive inside the mouse hook, so the list is only touched on the timer
    readonly (SafeFileHandle Handle, int Length)? settings;
    string lastSettings = "";
    long strokeX, strokeY;
    DateTime lastMove;
    double? lastDpi;
    int ticks;

    public MonitorForm(Remapper remapper)
    {
        this.remapper = remapper;
        Text = "Mouse Manager - Monitor";
        Size = new Size(820, 520);
        StartPosition = FormStartPosition.CenterScreen;

        var clear = new Button { Text = "Clear", AutoSize = true };
        clear.Click += (_, _) => events.Items.Clear();
        var copy = new Button { Text = "Copy all", AutoSize = true };
        copy.Click += (_, _) => { if (events.Items.Count > 0) Clipboard.SetText(string.Join(Environment.NewLine, events.Items.Cast<string>())); };
        var top = new FlowLayoutPanel { Dock = DockStyle.Top, AutoSize = true, Padding = new Padding(6) };
        top.Controls.AddRange([clear, copy, allDevices]);

        var bottom = new FlowLayoutPanel { Dock = DockStyle.Bottom, AutoSize = true, Padding = new Padding(6) };
        bottom.Controls.AddRange([measure, distance, dpiLabel, settingsLabel]);
        bottom.SetFlowBreak(dpiLabel, true);

        Controls.Add(events); // Fill must be added before the docked bars
        Controls.Add(top);
        Controls.Add(bottom);

        settings = Native.OpenFeatureCollection(remapper.IsTarget);
        settingsLabel.Text = settings is null ? "Settings report: not found (is the mouse on its 2.4G dongle?)" : "Settings report: reading...";
        measure.CheckedChanged += (_, _) => { strokeX = strokeY = 0; lastMove = default; lastDpi = null; };
        remapper.RawInput += OnRawInput;
        timer.Tick += (_, _) => OnTick();
        timer.Start();
        FormClosed += (_, _) => { remapper.RawInput -= OnRawInput; timer.Dispose(); settings?.Handle.Dispose(); };
    }

    public static double Dpi(double counts, double cm) => counts / (cm / 2.54);

    void OnRawInput(byte[] raw, string device)
    {
        bool target = remapper.IsTarget(device);
        int h = Native.RawHeaderSize;
        if (target && measure.Checked && Native.RawType(raw) == Native.RIM_TYPEMOUSE && (BitConverter.ToUInt16(raw, h) & 1) == 0) // relative motion
        {
            int dx = BitConverter.ToInt32(raw, h + 12), dy = BitConverter.ToInt32(raw, h + 16);
            if (dx != 0 || dy != 0) { strokeX += dx; strokeY += dy; lastMove = DateTime.Now; }
        }
        if ((target || allDevices.Checked) && RawInputLogger.Describe(raw) is { } text)
            Add(target ? $"N230  {text}" : $"other {text}  {device}");
    }

    void OnTick()
    {
        if (lastMove != default && (DateTime.Now - lastMove).TotalMilliseconds > StrokeEndMs) EndStroke();
        if (++ticks % 5 == 0) PollSettings();
        if (buffered.Count == 0) return;
        events.BeginUpdate();
        foreach (var line in buffered) events.Items.Add(line);
        buffered.Clear();
        while (events.Items.Count > MaxLines) events.Items.RemoveAt(0);
        events.EndUpdate();
        events.TopIndex = events.Items.Count - 1;
    }

    void EndStroke()
    {
        double counts = Math.Sqrt((double)strokeX * strokeX + (double)strokeY * strokeY); // net displacement, so wobble cancels out
        strokeX = strokeY = 0;
        lastMove = default;
        if (counts < MinStrokeCounts) return;
        double dpi = Dpi(counts, (double)distance.Value);
        string jump = lastDpi is { } prev && Math.Abs(dpi - prev) / prev > DpiJump ? $"   <-- DPI CHANGED (was ~{prev:0})" : "";
        lastDpi = dpi;
        dpiLabel.Text = $"cm.   Last slide: {counts:0} counts = ~{dpi:0} DPI";
        Add($"N230  SLIDE {counts:0} counts over {distance.Value} cm = ~{dpi:0} DPI{jump}");
    }

    void PollSettings()
    {
        if (settings is not { } s) return;
        if (Native.GetFeature(s.Handle, SettingsReportId, s.Length) is not { } report)
        {
            settingsLabel.Text = "Settings report: unavailable (mouse asleep or disconnected)";
            return;
        }
        string now = Convert.ToHexString(report);
        settingsLabel.Text = $"Settings report {SettingsReportId}: {now}";
        if (lastSettings != "" && now != lastSettings) Add($"N230  SETTINGS REPORT {lastSettings} -> {now}   <-- possible DPI change");
        lastSettings = now;
    }

    void Add(string line) => buffered.Add($"{DateTime.Now:HH:mm:ss.fff} {line}");
}
