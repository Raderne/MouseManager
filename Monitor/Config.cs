using System.Text.Json;

sealed class Config
{
    // Substrings of the Raw Input device path. Add the Bluetooth/wired IDs here (find them with --log).
    public List<string> Devices { get; set; } = [RawInputLogger.N230];
    public Dictionary<string, string> Buttons { get; set; } = new() { ["Middle"] = "Default", ["Back"] = "Default", ["Forward"] = "Default" };

    public static readonly string FilePath = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData), "MouseManager", "config.json");
    static readonly JsonSerializerOptions Indented = new() { WriteIndented = true };

    public static Config Load()
    {
        try { return JsonSerializer.Deserialize<Config>(File.ReadAllText(FilePath)) ?? new(); }
        catch (Exception ex) when (ex is IOException or JsonException) { return new(); }
    }

    public void Save()
    {
        Directory.CreateDirectory(Path.GetDirectoryName(FilePath)!);
        File.WriteAllText(FilePath, JsonSerializer.Serialize(this, Indented));
    }
}
