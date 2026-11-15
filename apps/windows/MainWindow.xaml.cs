using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;

namespace Layon;

public sealed partial class MainWindow : Window
{
    private readonly LayonCli cli = new();
    private readonly List<Button> actionButtons = [];

    public MainWindow()
    {
        InitializeComponent();
        AppWindow.Resize(new Windows.Graphics.SizeInt32(720, 640));
        _ = RefreshAsync();
    }

    private async void OnRefresh(object sender, RoutedEventArgs e) => await RefreshAsync();

    private async void OnPair(object sender, RoutedEventArgs e)
    {
        var input = new TextBox { PlaceholderText = "layon://pair/…", AcceptsReturn = false };
        var dialog = new ContentDialog
        {
            XamlRoot = Content.XamlRoot,
            Title = "Pair with a machine",
            Content = new StackPanel
            {
                Spacing = 12,
                Children =
                {
                    new TextBlock
                    {
                        Text = "Run `layon pair` on the machine and paste the text under its QR code.",
                        TextWrapping = TextWrapping.Wrap,
                    },
                    input,
                },
            },
            PrimaryButtonText = "Pair",
            CloseButtonText = "Cancel",
            DefaultButton = ContentDialogButton.Primary,
        };
        if (await dialog.ShowAsync() != ContentDialogResult.Primary) return;
        await ActAsync("Pairing…", "Paired.", () => cli.PairAsync(input.Text));
    }

    private async Task RefreshAsync()
    {
        List<Host> hosts;
        try
        {
            hosts = await cli.HostsAsync();
        }
        catch (Exception e)
        {
            Status.Text = e.Message;
            return;
        }

        HostList.Children.Clear();
        actionButtons.Clear();
        if (hosts.Count == 0)
        {
            HostList.Children.Add(new TextBlock
            {
                Text = "No paired machine. Run `layon pair` on it, then Pair… here.",
                TextWrapping = TextWrapping.Wrap,
            });
            return;
        }
        foreach (var host in hosts) HostList.Children.Add(Card(host));
    }

    private Border Card(Host host)
    {
        var state = new TextBlock { Text = "checking…", Opacity = 0.7 };
        _ = CheckAsync(host, state);

        var buttons = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
        buttons.Children.Add(ActionButton("Sleep", () => ActAsync($"Putting {host.Name} to sleep…", "Done.", () => cli.SleepAsync(host.Id))));
        buttons.Children.Add(ActionButton("Hibernate", () => ActAsync($"Hibernating {host.Name}…", "Done.", () => cli.HibernateAsync(host.Id))));
        buttons.Children.Add(ActionButton("Wake", () => ActAsync($"Waking {host.Name}…", "Awake.", () => cli.WakeAsync(host.Id))));
        buttons.Children.Add(ActionButton("Forget", () => ConfirmForgetAsync(host)));

        return new Border
        {
            Padding = new Thickness(16),
            CornerRadius = new CornerRadius(8),
            Background = (Brush)Application.Current.Resources["CardBackgroundFillColorDefaultBrush"],
            Child = new StackPanel
            {
                Spacing = 8,
                Children =
                {
                    new TextBlock
                    {
                        Text = $"{host.Name}  ({host.Os})",
                        Style = (Style)Application.Current.Resources["SubtitleTextBlockStyle"],
                    },
                    state,
                    buttons,
                },
            },
        };
    }

    private async Task CheckAsync(Host host, TextBlock state)
    {
        var awake = await cli.IsAwakeAsync(host.Id);
        state.Text = awake ? "awake" : "not answering";
    }

    private Button ActionButton(string label, Func<Task> onClick)
    {
        var button = new Button { Content = label, MinHeight = 44, MinWidth = 96 };
        button.Click += async (_, _) => await onClick();
        actionButtons.Add(button);
        return button;
    }

    private async Task ConfirmForgetAsync(Host host)
    {
        var dialog = new ContentDialog
        {
            XamlRoot = Content.XamlRoot,
            Title = $"Forget {host.Name}?",
            Content = "Pairing again needs its QR code.",
            PrimaryButtonText = "Forget",
            CloseButtonText = "Cancel",
        };
        if (await dialog.ShowAsync() != ContentDialogResult.Primary) return;
        await ActAsync($"Forgetting {host.Name}…", "Done.", () => cli.ForgetAsync(host.Id));
    }

    private async Task ActAsync(string doing, string done, Func<Task> action)
    {
        Status.Text = doing;
        SetEnabled(false);
        try
        {
            await action();
            Status.Text = done;
        }
        catch (Exception e)
        {
            Status.Text = e.Message;
        }
        finally
        {
            SetEnabled(true);
        }
        await RefreshAsync();
    }

    private void SetEnabled(bool enabled)
    {
        PairButton.IsEnabled = enabled;
        RefreshButton.IsEnabled = enabled;
        foreach (var button in actionButtons) button.IsEnabled = enabled;
    }
}
