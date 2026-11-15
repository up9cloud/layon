using System.Diagnostics;
using System.Text.Json;

namespace Layon;

public record Host(string Id, string Name, string Os);

public class CliException(string message) : Exception(message);

/// <summary>Every action is a run of the bundled <c>layon remote --json</c>.</summary>
public sealed class LayonCli
{
    private static readonly string Exe = Path.Combine(AppContext.BaseDirectory, "layon.exe");
    private static readonly JsonSerializerOptions Json = new() { PropertyNameCaseInsensitive = true };

    // One at a time: concurrent runs would race on the same client file.
    private readonly SemaphoreSlim gate = new(1, 1);

    public async Task<List<Host>> HostsAsync() =>
        JsonSerializer.Deserialize<List<Host>>(await RunAsync("hosts"), Json) ?? [];

    public async Task<Host> PairAsync(string ticket) =>
        JsonSerializer.Deserialize<Host>(await RunAsync("pair", ticket.Trim()), Json)
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
