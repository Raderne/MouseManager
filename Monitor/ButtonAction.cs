using System.Diagnostics;

// Parses a mapping like "Default", "Disabled", "Back", "Ctrl+Shift+T", "VolumeUp" or "run:calc.exe".
static class ButtonAction
{
    public sealed record Handlers(Action Down, Action Up);

    public static readonly string[] MouseButtons = ["Left", "Right", "Middle", "Back", "Forward"];

    // null = Default (leave the click alone). Throws FormatException for anything it can't parse.
    public static Handlers? Parse(string text)
    {
        text = text.Trim();
        if (text == "" || text.Equals("Default", StringComparison.OrdinalIgnoreCase)) return null;
        if (text.Equals("Disabled", StringComparison.OrdinalIgnoreCase)) return new(() => { }, () => { });

        if (text.StartsWith("run:", StringComparison.OrdinalIgnoreCase))
        {
            var target = text[4..].Trim();
            if (target == "") throw new FormatException("run: needs a program, file or URL");
            return new(() => Process.Start(new ProcessStartInfo(target) { UseShellExecute = true })?.Dispose(), () => { });
        }

        int mouse = Array.FindIndex(MouseButtons, b => b.Equals(text, StringComparison.OrdinalIgnoreCase));
        if (mouse >= 0) return new(() => Native.SendMouseButton(mouse, true), () => Native.SendMouseButton(mouse, false));

        // Key combo: press in order on button down, release in reverse on button up (so holding works).
        var keys = text.Split('+', StringSplitOptions.TrimEntries).Select(ParseKey).ToArray();
        return new(
            () => { foreach (var k in keys) Native.SendKey(k, true); },
            () => { for (int i = keys.Length - 1; i >= 0; i--) Native.SendKey(keys[i], false); });
    }

    public static Keys ParseKey(string token) => token.ToLowerInvariant() switch
    {
        "ctrl" or "control" => Keys.ControlKey,
        "shift" => Keys.ShiftKey,
        "alt" => Keys.Menu,
        "win" => Keys.LWin,
        "backspace" => Keys.Back,
        [var c] when char.IsAsciiDigit(c) => Keys.D0 + (c - '0'),
        _ when !char.IsAsciiDigit(token.FirstOrDefault()) && Enum.TryParse<Keys>(token, true, out var k)
               && Enum.IsDefined(k) && (k & Keys.Modifiers) == 0 => k,
        _ => throw new FormatException($"Unknown key '{token}'"),
    };
}
