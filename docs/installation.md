# Installation

zproxy ships prebuilt binaries with every GitHub Release. Downloading one of those assets is the
recommended way to install; building from source is only needed for development.

## Release assets

Each release publishes one static asset per platform:

| Asset | Platform |
|---|---|
| `zproxy-macos-aarch64` | macOS on Apple Silicon (M1/M2/M3/M4) |
| `zproxy-macos-x86_64` | macOS on Intel |
| `zproxy-linux-x86_64` | Linux on x86_64 |
| `zproxy-linux-aarch64` | Linux on arm64 |

They are plain executables — no archive, no installer. Releases live at
<https://github.com/jgermade/zproxy/releases>.

Identify your platform if you are unsure:

```bash
uname -s        # Darwin | Linux
uname -m        # arm64 / aarch64 | x86_64
```

## One-line install

[install.sh](../install.sh) detects the platform, downloads the matching asset, installs it into
`~/.local/bin` and adds the proxy block to your shell profile:

```bash
curl -fsSL https://raw.githubusercontent.com/jgermade/zproxy/main/install.sh | bash
```

```bash
wget -qO- https://raw.githubusercontent.com/jgermade/zproxy/main/install.sh | bash
```

Options are passed after `-s --`:

```bash
curl -fsSL https://raw.githubusercontent.com/jgermade/zproxy/main/install.sh \
  | bash -s -- --version v0.1.1 --dir /usr/local/bin --no-modify-profile
```

| Option | Environment variable | Default |
|---|---|---|
| `--version <vX.Y.Z>` | `ZPROXY_VERSION` | latest release |
| `--dir <path>` | `ZPROXY_INSTALL_DIR` | `~/.local/bin` |
| `--profile <path>` | `ZPROXY_PROFILE` | `~/.zshrc`, `~/.bashrc` or `~/.bash_profile` |
| `--no-modify-profile` | `ZPROXY_NO_MODIFY_PROFILE` | profile is updated |
| — | `GITHUB_TOKEN` / `ZPROXY_GITHUB_TOKEN` | unset (only needed for private forks or to avoid API rate limits) |

Behaviour worth knowing:

- The binary is staged inside the install directory and moved into place with an atomic rename, so
  upgrading works while an older zproxy is still running.
- The macOS quarantine attribute is cleared automatically.
- The downloaded binary is executed once (`zproxy --version`) before being installed.
- The shell profile is only touched when the `# --- zproxy ---` marker is missing, so re-running the
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

asset="zproxy-${os_tag}-${arch_tag}"
mkdir -p ~/.local/bin
curl -fsSL -o ~/.local/bin/zproxy \
  "https://github.com/jgermade/zproxy/releases/latest/download/${asset}"
chmod +x ~/.local/bin/zproxy
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
zproxy --version
zproxy paths
```

## Install a specific version

Replace `latest/download` with `download/<tag>`:

```bash
curl -fsSL -o ~/.local/bin/zproxy \
  https://github.com/jgermade/zproxy/releases/download/v0.1.0/zproxy-macos-aarch64
chmod +x ~/.local/bin/zproxy
```

## macOS: clear the quarantine flag

Binaries downloaded with a browser get the `com.apple.quarantine` attribute and Gatekeeper refuses
to run them. The release binaries are not notarised, so remove the attribute manually:

```bash
xattr -d com.apple.quarantine ~/.local/bin/zproxy
```

`curl` does not set the quarantine flag, so this step is only needed when downloading from Safari or
Chrome. If macOS still blocks the binary, allow it once from
**System Settings → Privacy & Security → Open Anyway**.

## System-wide install

Install into `/usr/local/bin` instead of `~/.local/bin` if you want it available for every user:

```bash
sudo install -m 0755 ~/Downloads/zproxy-macos-aarch64 /usr/local/bin/zproxy
```

Note that `zproxy service install` always registers a **user-level** service (LaunchAgent or
`systemd --user`), regardless of where the binary lives.

## Register the service

The service definition stores the absolute path of the binary that ran `service install`, so install
the binary to its final location **before** registering the service:

```bash
zproxy config           # create the configuration
zproxy service install  # register the user-level service
zproxy start            # start it
zproxy status           # verify
```

If you later move or replace the binary, re-register it:

```bash
zproxy service uninstall
zproxy service install
```

## Upgrade

Download the new asset over the existing binary and restart the service:

```bash
zproxy stop
curl -fsSL -o ~/.local/bin/zproxy \
  https://github.com/jgermade/zproxy/releases/latest/download/zproxy-macos-aarch64
chmod +x ~/.local/bin/zproxy
zproxy start
```

The binary path does not change, so the service definition stays valid.

## Uninstall

```bash
zproxy service uninstall          # remove the LaunchAgent / systemd unit
rm ~/.local/bin/zproxy            # remove the binary
rm -rf ~/.local/state/zproxy      # remove runtime state and logs
rm -rf ~/.config/zproxy           # remove the configuration
```

## Build from source

Only needed for development or unsupported platforms:

```bash
git clone https://github.com/jgermade/zproxy.git
cd zproxy
cargo build --release
install -m 0755 target/release/zproxy ~/.local/bin/zproxy
```

Do not point the service at `target/release/zproxy` inside the checkout: `cargo clean` or a rebuild
would leave the service with a broken or half-written executable.

## Next steps

- [shell-integration.md](shell-integration.md) — wire the proxy into zsh/bash.
- [quickstart.md](quickstart.md) — first run and basic usage.
- [configuration.md](configuration.md) — configuration reference.
