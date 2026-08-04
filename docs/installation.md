# Installation

localproxy ships prebuilt binaries with every GitHub Release. Downloading one of those assets is the
recommended way to install; building from source is only needed for development.

## Release assets

Each release publishes one static asset per platform:

| Asset | Platform |
|---|---|
| `localproxy-macos-aarch64` | macOS on Apple Silicon (M1/M2/M3/M4) |
| `localproxy-macos-x86_64` | macOS on Intel |
| `localproxy-linux-x86_64` | Linux on x86_64 |
| `localproxy-linux-aarch64` | Linux on arm64 |

They are plain executables — no archive, no installer. Releases live at
<https://github.com/jgermade/localproxy/releases>.

Identify your platform if you are unsure:

```bash
uname -s        # Darwin | Linux
uname -m        # arm64 / aarch64 | x86_64
```

## One-line install

[install.sh](../install.sh) detects the platform, downloads the matching asset, installs it into
`~/.local/bin` and adds the proxy block to your shell profile:

```bash
curl -fsSL https://raw.githubusercontent.com/jgermade/localproxy/main/install.sh | bash
```

```bash
wget -qO- https://raw.githubusercontent.com/jgermade/localproxy/main/install.sh | bash
```

Options are passed after `-s --`:

```bash
curl -fsSL https://raw.githubusercontent.com/jgermade/localproxy/main/install.sh \
  | bash -s -- --version v0.1.1 --dir /usr/local/bin --no-modify-profile
```

| Option | Environment variable | Default |
|---|---|---|
| `--version <vX.Y.Z>` | `LOCALPROXY_VERSION` | latest release |
| `--dir <path>` | `LOCALPROXY_INSTALL_DIR` | `~/.local/bin` |
| `--profile <path>` | `LOCALPROXY_PROFILE` | `~/.zshrc`, `~/.bashrc` or `~/.bash_profile` |
| `--no-modify-profile` | `LOCALPROXY_NO_MODIFY_PROFILE` | profile is updated |
| — | `GITHUB_TOKEN` / `LOCALPROXY_GITHUB_TOKEN` | unset (only needed for private forks or to avoid API rate limits) |

Behaviour worth knowing:

- The binary is staged inside the install directory and moved into place with an atomic rename, so
  upgrading works while an older localproxy is still running.
- The macOS quarantine attribute is cleared automatically.
- The downloaded binary is executed once (`localproxy --version`) before being installed.
- The shell profile is only touched when the `# --- localproxy ---` marker is missing, so re-running the
  installer never duplicates the block.
- Re-run the same command to upgrade.

The rest of this page describes the manual equivalent.

## Install the latest release manually

The snippet below detects the platform, downloads the matching asset from the latest release and
installs it into `~/.local/bin`:

```bash
set -e
os=$(uname -s)
arch=$(uname -m)

case "$os" in
  Darwin) os_tag=macos ;;
  Linux)  os_tag=linux ;;
  *) echo "unsupported OS: $os" >&2; exit 1 ;;
esac

case "$arch" in
  arm64|aarch64) arch_tag=aarch64 ;;
  x86_64|amd64)  arch_tag=x86_64 ;;
  *) echo "unsupported architecture: $arch" >&2; exit 1 ;;
esac

asset="localproxy-${os_tag}-${arch_tag}"
mkdir -p ~/.local/bin
curl -fsSL -o ~/.local/bin/localproxy \
  "https://github.com/jgermade/localproxy/releases/latest/download/${asset}"
chmod +x ~/.local/bin/localproxy
```

Make sure `~/.local/bin` is on your `PATH`:

```bash
# zsh
echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.zshrc

# bash
echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.bashrc
```

Verify:

```bash
localproxy --version
localproxy paths
```

## Install a specific version

Replace `latest/download` with `download/<tag>`:

```bash
curl -fsSL -o ~/.local/bin/localproxy \
  https://github.com/jgermade/localproxy/releases/download/v0.1.0/localproxy-macos-aarch64
chmod +x ~/.local/bin/localproxy
```

## macOS: clear the quarantine flag

Binaries downloaded with a browser get the `com.apple.quarantine` attribute and Gatekeeper refuses
to run them. The release binaries are not notarised, so remove the attribute manually:

```bash
xattr -d com.apple.quarantine ~/.local/bin/localproxy
```

`curl` does not set the quarantine flag, so this step is only needed when downloading from Safari or
Chrome. If macOS still blocks the binary, allow it once from
**System Settings → Privacy & Security → Open Anyway**.

## System-wide install

Install into `/usr/local/bin` instead of `~/.local/bin` if you want it available for every user:

```bash
sudo install -m 0755 ~/Downloads/localproxy-macos-aarch64 /usr/local/bin/localproxy
```

Note that `localproxy service install` always registers a **user-level** service (LaunchAgent or
`systemd --user`), regardless of where the binary lives.

## Register the service

The service definition stores the absolute path of the binary that ran `service install`, so install
the binary to its final location **before** registering the service:

```bash
localproxy config           # create the configuration
localproxy service install  # register the user-level service
localproxy start            # start it
localproxy status           # verify
```

If you later move or replace the binary, re-register it:

```bash
localproxy service uninstall
localproxy service install
```

## Upgrade

Download the new asset over the existing binary and restart the service:

```bash
localproxy stop
curl -fsSL -o ~/.local/bin/localproxy \
  https://github.com/jgermade/localproxy/releases/latest/download/localproxy-macos-aarch64
chmod +x ~/.local/bin/localproxy
localproxy start
```

The binary path does not change, so the service definition stays valid.

## Uninstall

```bash
localproxy service uninstall          # remove the LaunchAgent / systemd unit
rm ~/.local/bin/localproxy            # remove the binary
rm -rf ~/.local/state/localproxy      # remove runtime state and logs
rm -rf ~/.config/localproxy           # remove the configuration
```

## Build from source

Only needed for development or unsupported platforms:

```bash
git clone https://github.com/jgermade/localproxy.git
cd localproxy
cargo build --release
install -m 0755 target/release/localproxy ~/.local/bin/localproxy
```

Do not point the service at `target/release/localproxy` inside the checkout: `cargo clean` or a rebuild
would leave the service with a broken or half-written executable.

## Next steps

- [shell-integration.md](shell-integration.md) — wire the proxy into zsh/bash.
- [quickstart.md](quickstart.md) — first run and basic usage.
- [configuration.md](configuration.md) — configuration reference.
