using System.Diagnostics;
using System.Text.Json;
using System.Text.Json.Serialization;

namespace Layon;

public record Host(string Id, string Name, string Os);

// Source-generated, since reflection-based JSON does not survive Native AOT.
[JsonSourceGenerationOptions(PropertyNameCaseInsensitive = true)]
[JsonSerializable(typeof(Host))]
[JsonSerializable(typeof(List<Host>))]
internal partial class HostJson : JsonSerializerContext;

public class CliException(string message) : Exception(message);

/// <summary>Every action is a run of the bundled <c>layon remote --json</c>.</summary>
public sealed class LayonCli
{
    private static readonly string Exe = Path.Combine(AppContext.BaseDirectory, "layon.exe");

    // One at a time: concurrent runs would race on the same client file.
    private readonly SemaphoreSlim gate = new(1, 1);

    public async Task<List<Host>> HostsAsync() =>
        JsonSerializer.Deserialize(await RunAsync("hosts"), HostJson.Default.ListHost) ?? [];

    public async Task<Host> PairAsync(string ticket) =>
        JsonSerializer.Deserialize(await RunAsync("pair", ticket.Trim()), HostJson.Default.Host)
        ?? throw new CliException("layon printed no host");

    public async Task<bool> IsAwakeAsync(string id)
    {
        try
        {
            await RunAsync("info", id);
            return true;
        }
        catch (CliException)
        {
            return false;
        }
    }

    public Task SleepAsync(string id) => RunAsync("sleep", id);
    public Task HibernateAsync(string id) => RunAsync("hibernate", id);
    public Task WakeAsync(string id) => RunAsync("wake", id);
    public Task ForgetAsync(string id) => RunAsync("forget", id);

    private async Task<string> RunAsync(params string[] args)
    {
        await gate.WaitAsync();
        try
        {
            var start = new ProcessStartInfo(Exe)
            {
                RedirectStandardOutput = true,
                RedirectStandardError = true,
                UseShellExecute = false,
                CreateNoWindow = true,
            };
            start.ArgumentList.Add("remote");
            start.ArgumentList.Add("--json");
            foreach (var arg in args) start.ArgumentList.Add(arg);

            using var process = Process.Start(start) ?? throw new CliException($"could not start {Exe}");
            var stdout = process.StandardOutput.ReadToEndAsync();
            var stderr = process.StandardError.ReadToEndAsync();
            await process.WaitForExitAsync();
            if (process.ExitCode != 0) throw new CliException(ErrorMessage(await stderr));
            return await stdout;
        }
        finally
        {
            gate.Release();
        }
    }

    private static string ErrorMessage(string stderr)
    {
        var line = stderr.Split('\n').Select(l => l.Trim()).LastOrDefault(l => l.StartsWith('{'));
        if (line is null)
        {
            var text = stderr.Trim();
            return text.Length > 0 ? text : "layon exited with an error";
        }
        try
        {
            using var doc = JsonDocument.Parse(line);
            return doc.RootElement.GetProperty("error").GetString() ?? line;
        }
        catch (Exception e) when (e is JsonException or KeyNotFoundException)
        {
            return line;
        }
    }
}
