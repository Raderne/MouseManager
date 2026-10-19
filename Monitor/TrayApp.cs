using Microsoft.Win32;

sealed class TrayApp : ApplicationContext
{
    const string RunKey = @"Software\Microsoft\Windows\CurrentVersion\Run", AppName = "MouseManager";

    readonly NotifyIcon tray;
    readonly Remapper remapper;
    Config config = Config.Load();
    SettingsForm? settings;
    MonitorForm? monitor;

    public TrayApp()
    {
        tray = new NotifyIcon { Icon = SystemIcons.Application, Text = "Mouse Manager (Lenovo N230)", Visible = true, ContextMenuStrip = new ContextMenuStrip() };
        remapper = new Remapper(ex => tray.ShowBalloonTip(4000, "Mouse Manager", ex.Message, ToolTipIcon.Warning));
        try { remapper.Apply(config); }
        catch (FormatException ex)
        {
            tray.ShowBalloonTip(6000, "Mouse Manager: bad config, using defaults", ex.Message, ToolTipIcon.Warning);
            remapper.Apply(config = new Config());
        }

        var startup = new ToolStripMenuItem("Start with Windows") { CheckOnClick = true, Checked = Registry.CurrentUser.OpenSubKey(RunKey)?.GetValue(AppName) != null };
        startup.CheckedChanged += (_, _) =>
        {
            using var key = Registry.CurrentUser.CreateSubKey(RunKey);
            if (startup.Checked) key.SetValue(AppName, $"\"{Environment.ProcessPath}\"");
            else key.DeleteValue(AppName, false);
        };
        tray.ContextMenuStrip.Items.Add("Settings...", null, (_, _) => ShowSettings());
        tray.ContextMenuStrip.Items.Add("Monitor / DPI meter...", null, (_, _) => ShowMonitor());
        tray.ContextMenuStrip.Items.Add(startup);
        tray.ContextMenuStrip.Items.Add("Exit", null, (_, _) => ExitThread());
        tray.DoubleClick += (_, _) => ShowSettings();
    }

    void ShowSettings()
    {
        if (settings is { IsDisposed: false }) { settings.Activate(); return; }
        settings = new SettingsForm(config, c => { remapper.Apply(c); c.Save(); config = c; });
        settings.Show();
    }

    void ShowMonitor()
    {
        if (monitor is { IsDisposed: false }) { monitor.Activate(); return; }
        monitor = new MonitorForm(remapper);
        monitor.Show();
    }

    protected override void ExitThreadCore()
    {
        tray.Visible = false;
        tray.Dispose();
        remapper.Dispose();
        base.ExitThreadCore();
    }
}

sealed class SettingsForm : Form
{
    static readonly string[] Presets =
    [
        "Default", "Disabled", "Left", "Right", "Middle", "Back", "Forward",
        "Alt+Left", "Alt+Right", "Ctrl+C", "Ctrl+V", "Ctrl+Z", "Ctrl+W", "Ctrl+Shift+T", "Ctrl+Tab",
        "Win+Tab", "Win+D", "Win+Shift+S", "F5",
        "MediaPlayPause", "MediaNextTrack", "MediaPreviousTrack", "VolumeUp", "VolumeDown", "VolumeMute",
        "run:calc.exe",
    ];

    public SettingsForm(Config config, Action<Config> save)
    {
        Text = "Mouse Manager - Lenovo N230";
        FormBorderStyle = FormBorderStyle.FixedDialog;
        MaximizeBox = MinimizeBox = false;
        StartPosition = FormStartPosition.CenterScreen;
        AutoSize = true;
        AutoSizeMode = AutoSizeMode.GrowAndShrink;
        Padding = new Padding(12);

        var grid = new TableLayoutPanel { ColumnCount = 2, AutoSize = true, Dock = DockStyle.Fill };
        var boxes = new Dictionary<string, ComboBox>();
        foreach (var (button, label) in new[] { ("Middle", "Wheel click"), ("Back", "Side button - Back (X1)"), ("Forward", "Side button - Forward (X2)") })
        {
            var box = new ComboBox { Width = 280, DropDownStyle = ComboBoxStyle.DropDown };
            box.Items.AddRange(Presets);
            box.Text = config.Buttons.GetValueOrDefault(button, "Default");
            grid.Controls.Add(new Label { Text = label, AutoSize = true, Anchor = AnchorStyles.Left });
            grid.Controls.Add(box);
            boxes[button] = box;
        }

        var help = new Label
        {
            AutoSize = true, MaximumSize = new Size(460, 0), Margin = new Padding(3, 10, 3, 10),
            Text = "Pick a preset or type a key combo (Ctrl+Shift+T, Win+D, F5, VolumeUp), a mouse button (Left, Right, Middle, Back, Forward), " +
                   "Disabled, or run:<program, file or URL>. Default keeps the normal behaviour.",
        };
        grid.Controls.Add(help);
        grid.SetColumnSpan(help, 2);

        var devices = new TextBox { Multiline = true, Width = 280, Height = 50, Text = string.Join(Environment.NewLine, config.Devices) };
        grid.Controls.Add(new Label { Text = "Device IDs (one per line)", AutoSize = true, Anchor = AnchorStyles.Left });
        grid.Controls.Add(devices);

        var saveButton = new Button { Text = "Save", AutoSize = true, Anchor = AnchorStyles.Right };
        saveButton.Click += (_, _) =>
        {
            var updated = new Config
            {
                Devices = devices.Lines.Select(l => l.Trim()).Where(l => l != "").ToList(),
                Buttons = boxes.ToDictionary(kv => kv.Key, kv => kv.Value.Text.Trim()),
            };
            try { save(updated); Close(); }
            catch (FormatException ex) { MessageBox.Show(this, ex.Message, "Invalid mapping", MessageBoxButtons.OK, MessageBoxIcon.Warning); }
        };
        grid.Controls.Add(new Label());
        grid.Controls.Add(saveButton);
        Controls.Add(grid);
    }
}
