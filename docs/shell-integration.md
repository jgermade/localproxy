# Shell integration (zsh / bash)

zproxy does not touch the system network settings. Applications use it only when they are told to,
usually through the `http_proxy` / `https_proxy` environment variables. This page shows how to wire
that into zsh and bash.

Everything below assumes the default listen address `127.0.0.1:8888`. Check yours with:

```bash
zproxy status
```

## Where to put the snippets

| Shell | File |
|---|---|
| zsh | `~/.zshrc` |
| bash (Linux) | `~/.bashrc` |
| bash (macOS, login shell) | `~/.bash_profile` |

Reload after editing:

```bash
source ~/.zshrc     # or ~/.bashrc
```

## Always-on proxy variables

Simplest setup: export the variables unconditionally. Only do this if the daemon runs as a service,
otherwise every tool will fail when zproxy is down.

```bash
export ZPROXY_URL="http://127.0.0.1:8888"
export http_proxy="$ZPROXY_URL"
export https_proxy="$ZPROXY_URL"
export HTTP_PROXY="$ZPROXY_URL"
export HTTPS_PROXY="$ZPROXY_URL"
export no_proxy="localhost,127.0.0.1,::1"
export NO_PROXY="$no_proxy"
```

Both lowercase and uppercase forms are set because tools disagree: `curl` and most Unix tools read
the lowercase ones, while many language runtimes read the uppercase ones.

`no_proxy` must list everything that has to bypass the proxy. Extend it with your internal domains:

```bash
export no_proxy="localhost,127.0.0.1,::1,.internal.example.com,192.168.0.0/16"
```

## Toggle functions

Better default: keep the variables off and switch them on demand. Works in both zsh and bash.

```bash
ZPROXY_URL="http://127.0.0.1:8888"
ZPROXY_NO_PROXY="localhost,127.0.0.1,::1"

proxy-on() {
  export http_proxy="$ZPROXY_URL" https_proxy="$ZPROXY_URL"
  export HTTP_PROXY="$ZPROXY_URL" HTTPS_PROXY="$ZPROXY_URL"
  export no_proxy="$ZPROXY_NO_PROXY" NO_PROXY="$ZPROXY_NO_PROXY"
  echo "proxy on -> $ZPROXY_URL"
}

proxy-off() {
  unset http_proxy https_proxy HTTP_PROXY HTTPS_PROXY no_proxy NO_PROXY
  echo "proxy off"
}

proxy-status() {
  if [ -n "$http_proxy" ]; then
    echo "proxy on -> $http_proxy"
  else
    echo "proxy off"
  fi
  zproxy status 2>/dev/null || echo "daemon not running"
}
```

Usage:

```console
$ proxy-on
proxy on -> http://127.0.0.1:8888
$ curl -s https://example.com > /dev/null && echo ok
ok
$ proxy-off
proxy off
```

## Run a single command through the proxy

Avoids polluting the shell environment:

```bash
with-proxy() {
  http_proxy="http://127.0.0.1:8888" https_proxy="http://127.0.0.1:8888" \
  HTTP_PROXY="http://127.0.0.1:8888" HTTPS_PROXY="http://127.0.0.1:8888" \
  "$@"
}
```

```bash
with-proxy curl -s https://example.com
with-proxy npm install
```

## Enable only when the daemon is listening

Prevents a broken shell when zproxy is stopped. Add to `~/.zshrc` / `~/.bashrc`:

```bash
if command -v zproxy > /dev/null 2>&1 && zproxy status > /dev/null 2>&1; then
  export http_proxy="http://127.0.0.1:8888"
  export https_proxy="$http_proxy"
  export HTTP_PROXY="$http_proxy"
  export HTTPS_PROXY="$http_proxy"
  export no_proxy="localhost,127.0.0.1,::1"
  export NO_PROXY="$no_proxy"
fi
```

`zproxy status` talks to the control socket, so it succeeds only when the daemon is actually
running. It adds a few milliseconds to shell startup; use the toggle functions instead if that
matters.

## Start the daemon from the shell

If you do not want a registered service, start it on demand:

```bash
zproxy-up() {
  zproxy status > /dev/null 2>&1 || zproxy start --detached
  proxy-on
}

zproxy-down() {
  proxy-off
  zproxy stop > /dev/null 2>&1
}
```

For an always-running daemon, prefer the service (see
[operations.md](operations.md)):

```bash
zproxy service install
zproxy start
```

## Useful aliases

```bash
alias zpstatus='zproxy status'
alias zplogs='zproxy logs --follow'
alias zpconfig='zproxy config'
alias zprestart='zproxy service restart'
```

## Tools that ignore the environment variables

Some tools need their own configuration:

```bash
# git
git config --global http.proxy http://127.0.0.1:8888
git config --global https.proxy http://127.0.0.1:8888
git config --global --unset http.proxy      # revert

# npm
npm config set proxy http://127.0.0.1:8888
npm config set https-proxy http://127.0.0.1:8888
npm config delete proxy                     # revert

# Docker CLI: ~/.docker/config.json ("proxies" section)
# Homebrew, pip, cargo and most CLIs honour http_proxy/https_proxy.
```

`ssh` does not read `http_proxy`. Route it through the CONNECT tunnel explicitly in `~/.ssh/config`:

```sshconfig
Host github.com
  ProxyCommand nc -X connect -x 127.0.0.1:8888 %h %p
```

## Verify the integration

```bash
env | grep -i proxy
curl -v http://example.com 2>&1 | head -5     # should show the proxy connection
zproxy logs --lines 20
```

If a request does not go through zproxy, the usual causes are:

- The variables are exported in a subshell only.
- The host matches `no_proxy`.
- The tool uses its own proxy settings (see the section above).
